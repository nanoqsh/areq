use {
    crate::buffer::Buffer,
    flate2::{DecompressError, FlushDecompress, Status},
    std::mem,
};

#[derive(Clone, Copy)]
struct Flags(u8);

impl Flags {
    #[expect(dead_code)]
    const ASCII: u8 = 1 << 0;
    const CRC: u8 = 1 << 1;
    const EXTRA: u8 = 1 << 2;
    const NAME: u8 = 1 << 3;
    const COMMENT: u8 = 1 << 4;

    fn has(self, bit: u8) -> bool {
        (self.0 & bit) != 0
    }
}

struct Header {
    flags: Flags,
    mtime: u32,
    extra: Box<[u8]>,
    name: Vec<u8>,
    comment: Vec<u8>,
    crc: u16,
}

impl Header {
    fn empty() -> Self {
        Self {
            flags: Flags(0),
            mtime: 0,
            extra: Box::new([]),
            name: vec![],
            comment: vec![],
            crc: 0,
        }
    }
}

struct Footer {
    crc: u32,
    isize: u32,
}

impl Footer {
    fn empty() -> Self {
        Self { crc: 0, isize: 0 }
    }
}

enum State {
    Start(Buffer<[u8; 10]>),
    ExtraLen(Buffer<[u8; 2]>),
    Extra(Buffer<Box<[u8]>>),
    Name(Vec<u8>),
    Comment(Vec<u8>),
    Crc(Buffer<[u8; 2]>),
    Payload,
    Footer(Buffer<[u8; 8]>),
}

impl State {
    fn header_is_ready(&self) -> bool {
        matches!(self, Self::Payload | Self::Footer(_))
    }
}

struct Parser {
    state: State,
    header: Header,
    footer: Footer,
}

impl Parser {
    fn new() -> Self {
        let state = State::Start(Buffer::default());
        let header = Header::empty();
        let footer = Footer::empty();

        Self {
            state,
            header,
            footer,
        }
    }

    fn header(&self) -> Option<&Header> {
        if self.state.header_is_ready() {
            Some(&self.header)
        } else {
            None
        }
    }

    fn parse<D>(&mut self, input: &mut &[u8], mut deco: D) -> Out
    where
        D: FnMut(&mut &[u8]) -> bool,
    {
        loop {
            match &mut self.state {
                State::Start(buf) => {
                    let Some(&mut bytes) = buf.read_from(input) else {
                        return Out::Running;
                    };

                    let Some((flags, mtime)) = parse_start(bytes) else {
                        return Out::InvalidHeader;
                    };

                    self.header.flags = flags;
                    self.header.mtime = mtime;
                    self.state = State::ExtraLen(Buffer::default());
                }
                State::ExtraLen(buf) => {
                    if !self.header.flags.has(Flags::EXTRA) {
                        self.state = State::Name(vec![]);
                        continue;
                    }

                    let Some(&mut bytes) = buf.read_from(input) else {
                        return Out::Running;
                    };

                    let len = u16::from_le_bytes(bytes);
                    self.state = State::Extra(Buffer::alloc(len as usize));
                }
                State::Extra(buf) => {
                    let Some(extra) = buf.read_from(input) else {
                        return Out::Running;
                    };

                    mem::swap(&mut self.header.extra, extra);
                    self.state = State::Name(vec![]);
                }
                State::Name(out) => {
                    if !self.header.flags.has(Flags::NAME) {
                        self.state = State::Comment(vec![]);
                        continue;
                    }

                    let (read, parse) = read_while(0, input);
                    out.extend_from_slice(read);
                    if parse {
                        return Out::Running;
                    }

                    mem::swap(&mut self.header.name, out);
                    self.state = State::Comment(vec![]);
                }
                State::Comment(out) => {
                    if !self.header.flags.has(Flags::COMMENT) {
                        self.state = State::Crc(Buffer::default());
                        continue;
                    }

                    let (read, parse) = read_while(0, input);
                    out.extend_from_slice(read);
                    if parse {
                        return Out::Running;
                    }

                    mem::swap(&mut self.header.comment, out);
                    self.state = State::Crc(Buffer::default());
                }
                State::Crc(buf) => {
                    if !self.header.flags.has(Flags::CRC) {
                        self.state = State::Payload;
                        continue;
                    }

                    let Some(&mut bytes) = buf.read_from(input) else {
                        return Out::Running;
                    };

                    self.header.crc = u16::from_le_bytes(bytes);
                    self.state = State::Payload;
                }
                State::Payload => {
                    if deco(input) {
                        return Out::Running;
                    }

                    self.state = State::Footer(Buffer::default());
                }
                State::Footer(buf) => {
                    let Some(&mut bytes) = buf.read_from(input) else {
                        return Out::Running;
                    };

                    self.footer = parse_footer(bytes);
                    return Out::Done;
                }
            }
        }
    }
}

enum Out {
    Running,
    Done,
    InvalidHeader,
}

fn parse_start(s: [u8; 10]) -> Option<(Flags, u32)> {
    let [31, 139, 8, flags, mt3, mt2, mt1, mt0, xfl, os] = s else {
        return None;
    };

    let flags = Flags(flags);
    let mtime = u32::from_le_bytes([mt3, mt2, mt1, mt0]);
    _ = xfl; // ignored
    _ = os; // ignored

    Some((flags, mtime))
}

fn parse_footer(s: [u8; 8]) -> Footer {
    let [c3, c2, c1, c0, i3, i2, i1, i0] = s;
    let crc = u32::from_le_bytes([c3, c2, c1, c0]);
    let isize = u32::from_le_bytes([i3, i2, i1, i0]);
    Footer { crc, isize }
}

fn read_while<'input>(u: u8, input: &mut &'input [u8]) -> (&'input [u8], bool) {
    match memchr::memchr(u, input) {
        Some(n) => {
            let (left, right) = input.split_at(n);
            *input = &right[1..];
            (left, false)
        }
        None => {
            let out = *input;
            *input = &[];
            (out, true)
        }
    }
}

struct Decoder {
    deco: flate2::Decompress,
    parser: Parser,
}

impl Decoder {
    fn new() -> Self {
        Self {
            deco: flate2::Decompress::new(false),
            parser: Parser::new(),
        }
    }

    fn decode(&mut self, input: &mut &[u8], output: &mut [u8]) -> Result<Decoded, Error> {
        debug_assert!(!output.is_empty(), "do not call decode with empty output");

        let mut decoded = Decoded {
            read_decompressed: 0,
            end: false,
        };

        let mut err = Ok(());

        let deco = |input: &mut &[u8]| {
            let input_size = self.deco.total_in();
            let output_size = self.deco.total_out();

            let res = self.deco.decompress(input, output, FlushDecompress::None);

            let consumed = self.deco.total_in() - input_size;
            *input = &input[consumed as usize..];

            decoded.read_decompressed = (self.deco.total_out() - output_size) as usize;
            if decoded.read_decompressed == 0 {
                decoded.end = true;
            }

            match res {
                Ok(Status::Ok) => true,
                Ok(Status::BufError) => todo!("ask more input"),
                Ok(Status::StreamEnd) => {
                    decoded.end = true;
                    false
                }
                Err(e) => {
                    err = Err(Error::Decompress(e));
                    true
                }
            }
        };

        match self.parser.parse(input, deco) {
            Out::Running => err.map(|_| decoded),
            Out::Done => Ok(decoded),
            Out::InvalidHeader => Err(Error::InvalidHeader),
        }
    }
}

struct Decoded {
    read_decompressed: usize,
    end: bool,
}

#[derive(Debug)]
enum Error {
    InvalidHeader,
    Decompress(DecompressError),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode() {
        let mut d = Decoder::new();

        let mut input = include_bytes!("../test/hello.gzip").as_slice();
        let mut output = [0; 20];
        let decoded = d.decode(&mut input, &mut output).expect("decode input");

        let expected = "hello world!";
        assert_eq!(decoded.read_decompressed, expected.len());
        assert!(decoded.end);
        assert!(output.starts_with(expected.as_bytes()));
    }

    #[test]
    fn decode_partial() {
        let mut d = Decoder::new();

        let mut output = [0; 20];
        let mut pos = 0;

        let input = include_bytes!("../test/hello.gzip").as_slice();
        for mut part in input.chunks(4) {
            let decoded = d
                .decode(&mut part, &mut output[pos..])
                .expect("decode input");

            pos += decoded.read_decompressed;
        }

        let expected = "hello world!";
        assert!(output.starts_with(expected.as_bytes()));
    }
}
