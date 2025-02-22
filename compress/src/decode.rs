use {
    crate::buffer::Buffer,
    flate2::{Crc, DecompressError, FlushDecompress, Status},
    std::{error, fmt, mem, ops::ControlFlow},
};

#[derive(Clone, Copy, Debug)]
struct Flags(u8);

impl Flags {
    const CRC: u8 = 1 << 1;
    const EXTRA: u8 = 1 << 2;
    const NAME: u8 = 1 << 3;
    const COMMENT: u8 = 1 << 4;

    fn has(self, bit: u8) -> bool {
        (self.0 & bit) != 0
    }
}

#[derive(Debug)]
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

#[derive(Debug)]
struct Footer {
    crc: u32,
    isize: u32,
}

impl Footer {
    fn empty() -> Self {
        Self { crc: 0, isize: 0 }
    }

    fn checksum(&self, crc: &Crc) -> bool {
        self.crc == crc.sum() && self.isize == crc.amount()
    }
}

#[derive(Debug)]
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
    fn is_header_ready(&self) -> bool {
        matches!(self, Self::Payload | Self::Footer(_))
    }
}

#[derive(Debug)]
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

    #[expect(dead_code)]
    fn header(&self) -> Option<&Header> {
        if self.state.is_header_ready() {
            Some(&self.header)
        } else {
            None
        }
    }

    fn parse<D>(&mut self, input: &mut &[u8], mut deco: D) -> Parsed
    where
        D: FnMut(&mut &[u8]) -> ControlFlow<()>,
    {
        loop {
            match &mut self.state {
                State::Start(buf) => {
                    let Some(&mut bytes) = buf.read_from(input) else {
                        return Parsed::Done;
                    };

                    let Some((flags, mtime)) = parse_start(bytes) else {
                        return Parsed::InvalidHeader;
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
                        return Parsed::Done;
                    };

                    let len = u16::from_le_bytes(bytes);
                    self.state = State::Extra(Buffer::alloc(len as usize));
                }
                State::Extra(buf) => {
                    let Some(extra) = buf.read_from(input) else {
                        return Parsed::Done;
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
                        return Parsed::Done;
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
                        return Parsed::Done;
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
                        return Parsed::Done;
                    };

                    self.header.crc = u16::from_le_bytes(bytes);
                    self.state = State::Payload;
                }
                State::Payload => match deco(input) {
                    ControlFlow::Continue(()) => return Parsed::Done,
                    ControlFlow::Break(()) => self.state = State::Footer(Buffer::default()),
                },
                State::Footer(buf) => {
                    let Some(&mut bytes) = buf.read_from(input) else {
                        return Parsed::Done;
                    };

                    self.footer = parse_footer(bytes);
                    return Parsed::End;
                }
            }
        }
    }
}

enum Parsed {
    Done,
    End,
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

#[derive(Debug)]
pub struct Decoder {
    deco: flate2::Decompress,
    parser: Parser,
    crc: Crc,
}

impl Decoder {
    pub fn decode(&mut self, input: &mut &[u8], output: &mut [u8]) -> Decoded {
        let mut written = 0;
        let mut need_more_input = false;
        let mut err = None;

        let deco = |input: &mut &[u8]| {
            let input_size = self.deco.total_in();
            let output_size = self.deco.total_out();

            let res = self.deco.decompress(input, output, FlushDecompress::None);

            let read_input = self.deco.total_in() - input_size;
            *input = &input[read_input as usize..];

            written = (self.deco.total_out() - output_size) as usize;
            self.crc.update(&output[..written]);

            match res {
                Ok(Status::Ok) => ControlFlow::Continue(()),
                Ok(Status::BufError) => {
                    need_more_input = true;
                    ControlFlow::Continue(())
                }
                Ok(Status::StreamEnd) => ControlFlow::Break(()),
                Err(e) => {
                    err = Some(Error::Decompress(e));
                    ControlFlow::Continue(())
                }
            }
        };

        match self.parser.parse(input, deco) {
            Parsed::Done if need_more_input => Decoded::NeedMoreInput,
            Parsed::Done => err.map_or(
                Decoded::Done {
                    written,
                    end: false,
                },
                Decoded::Fail,
            ),
            Parsed::End if self.parser.footer.checksum(&self.crc) => {
                Decoded::Done { written, end: true }
            }
            Parsed::End => Decoded::Fail(Error::ChecksumMismatch),
            Parsed::InvalidHeader => Decoded::Fail(Error::InvalidHeader),
        }
    }
}

impl Default for Decoder {
    fn default() -> Self {
        Self {
            deco: flate2::Decompress::new(false),
            parser: Parser::new(),
            crc: Crc::default(),
        }
    }
}

#[derive(Debug)]
pub enum Decoded {
    Done { written: usize, end: bool },
    NeedMoreInput,
    Fail(Error),
}

#[derive(Debug)]
pub enum Error {
    InvalidHeader,
    ChecksumMismatch,
    Decompress(DecompressError),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidHeader => write!(f, "invalid header"),
            Self::ChecksumMismatch => write!(f, "the checksum doesn't match"),
            Self::Decompress(e) => write!(f, "decompress error: {e}"),
        }
    }
}

impl error::Error for Error {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        match self {
            Self::InvalidHeader | Self::ChecksumMismatch => None,
            Self::Decompress(e) => Some(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode(expected: &[u8], mut input: &[u8]) {
        let mut d = Decoder::default();
        let mut output = vec![0; expected.len()];
        let Decoded::Done { written, end } = d.decode(&mut input, output.as_mut_slice()) else {
            panic!("failed to decode input");
        };

        assert_eq!(written, expected.len());
        assert!(end);
        assert!(input.is_empty());
        assert_eq!(output, expected);
    }

    #[test]
    fn decode_hello() {
        decode(
            include_bytes!("../test/hello.txt"),
            include_bytes!("../test/hello.gzip"),
        );
    }

    #[test]
    fn decode_lorem() {
        decode(
            include_bytes!("../test/lorem.txt"),
            include_bytes!("../test/lorem.gzip"),
        );
    }

    fn decode_partial(expected: &[u8], input: &[u8]) {
        let mut d = Decoder::default();
        let mut output = vec![0; expected.len()];
        let mut p = 0;
        let mut ended = false;

        for mut part in input.chunks(4) {
            let Decoded::Done { written, end } = d.decode(&mut part, &mut output[p..]) else {
                panic!("failed to decode input");
            };

            p += written;
            ended = end || ended;

            assert!(part.is_empty());
        }

        assert_eq!(p, expected.len());
        assert!(ended);
        assert_eq!(output, expected);
    }

    #[test]
    fn decode_partial_hello() {
        decode_partial(
            include_bytes!("../test/hello.txt"),
            include_bytes!("../test/hello.gzip"),
        );
    }

    #[test]
    fn decode_partial_lorem() {
        decode_partial(
            include_bytes!("../test/lorem.txt"),
            include_bytes!("../test/lorem.gzip"),
        );
    }

    #[test]
    fn decode_half_input() {
        let expected = include_bytes!("../test/hello.txt");
        let input = include_bytes!("../test/hello.gzip");
        let mut input = &input[..input.len() / 2];

        let mut d = Decoder::default();
        let mut output = vec![0; expected.len()];
        let decoded = d.decode(&mut input, output.as_mut_slice());
        assert!(matches!(decoded, Decoded::Done { end: false, .. }));

        let decoded = d.decode(&mut input, output.as_mut_slice());
        assert!(matches!(decoded, Decoded::NeedMoreInput));
    }

    #[test]
    fn decode_checksum_mismatch() {
        let expected = include_bytes!("../test/hello.txt");
        let mut input = const {
            let mut input = *include_bytes!("../test/hello.gzip");
            input[input.len() - 5] = 0;
            input
        }
        .as_slice();

        let mut d = Decoder::default();
        let mut output = vec![0; expected.len()];
        let decoded = d.decode(&mut input, output.as_mut_slice());
        assert!(matches!(decoded, Decoded::Fail(Error::ChecksumMismatch)));
    }
}
