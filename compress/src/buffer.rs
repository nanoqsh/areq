#[derive(Debug)]
pub(crate) struct Buffer<C> {
    rest: usize,
    data: C,
}

impl<C> Buffer<C> {
    pub fn just(self) -> Buffer<Maybe<C>> {
        Buffer {
            rest: self.rest,
            data: Maybe::Just(self.data),
        }
    }

    pub fn read_from(&mut self, input: &mut &[u8]) -> Option<&mut C>
    where
        C: Consume,
    {
        let n = usize::min(self.rest, input.len());
        let (left, right) = input.split_at(n);
        self.data.consume(self.rest, left);
        *input = right;

        self.rest -= n;
        if self.rest == 0 {
            Some(&mut self.data)
        } else {
            None
        }
    }
}

impl<C> Buffer<Maybe<C>> {
    pub fn nothing(rest: usize) -> Self {
        let data = Maybe::Nothing;
        Self { rest, data }
    }
}

impl Buffer<Box<[u8]>> {
    pub fn alloc(rest: usize) -> Self {
        let data = Box::from(vec![0; rest]);
        Self { rest, data }
    }
}

impl<C> Default for Buffer<C>
where
    C: Default + AsMut<[u8]>,
{
    fn default() -> Self {
        let mut data = C::default();
        let rest = data.as_mut().len();
        Self { rest, data }
    }
}

pub(crate) trait Consume {
    fn consume(&mut self, rest: usize, input: &[u8]);
}

impl<C> Consume for C
where
    C: AsMut<[u8]>,
{
    fn consume(&mut self, rest: usize, input: &[u8]) {
        let inner = self.as_mut();
        let step = inner.len() - rest;
        inner[step..step + input.len()].copy_from_slice(input);
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Maybe<C> {
    Just(C),
    Nothing,
}

impl<C> Consume for Maybe<C>
where
    C: Consume,
{
    fn consume(&mut self, rest: usize, input: &[u8]) {
        match self {
            Self::Just(c) => c.consume(rest, input),
            Self::Nothing => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buffer_read() {
        let mut b: Buffer<[u8; 4]> = Buffer::default();

        let mut input = [0, 1].as_slice();
        assert!(b.read_from(&mut input).is_none());
        assert_eq!(b.rest, 2);
        assert!(input.is_empty());

        let mut input = [2, 3].as_slice();
        assert_eq!(b.read_from(&mut input), Some(&mut [0, 1, 2, 3]));
        assert_eq!(b.rest, 0);
        assert!(input.is_empty());

        let mut input = [4, 5].as_slice();
        assert_eq!(b.read_from(&mut input), Some(&mut [0, 1, 2, 3]));
        assert_eq!(b.rest, 0);
        assert_eq!(input, [4, 5]);
    }

    #[test]
    fn buffer_read_more_input() {
        let mut b: Buffer<[u8; 4]> = Buffer::default();

        let mut input = [0, 1, 2, 3, 4, 5].as_slice();
        assert_eq!(b.read_from(&mut input), Some(&mut [0, 1, 2, 3]));
        assert_eq!(b.rest, 0);
        assert_eq!(input, [4, 5]);
    }

    #[test]
    fn buffer_read_nothing() {
        let mut b: Buffer<Maybe<[u8; 0]>> = Buffer::nothing(4);

        let mut input = [0, 1].as_slice();
        assert!(b.read_from(&mut input).is_none());
        assert_eq!(b.rest, 2);
        assert!(input.is_empty());

        let mut input = [2, 3].as_slice();
        assert_eq!(b.read_from(&mut input), Some(&mut Maybe::Nothing));
        assert_eq!(b.rest, 0);
        assert!(input.is_empty());
    }
}
