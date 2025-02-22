#[derive(Debug, Default)]
pub(crate) struct Buffer<B> {
    data: B,
    len: usize,
}

impl<B> Buffer<B>
where
    B: AsMut<[u8]>,
{
    pub fn read_from(&mut self, input: &mut &[u8]) -> Option<&mut B> {
        let unfilled = self.unfilled();
        let n = usize::min(input.len(), unfilled.len());
        let (left, right) = input.split_at(n);
        unfilled[..n].copy_from_slice(left);
        *input = right;

        self.len += n;
        self.filled()
    }

    fn filled(&mut self) -> Option<&mut B> {
        if self.unfilled().is_empty() {
            Some(&mut self.data)
        } else {
            None
        }
    }

    fn unfilled(&mut self) -> &mut [u8] {
        &mut self.data.as_mut()[self.len..]
    }
}

impl Buffer<Box<[u8]>> {
    pub fn alloc(len: usize) -> Self {
        Self {
            data: Box::from(vec![0; len]),
            len: 0,
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
        assert_eq!(b.len, 2);
        assert!(input.is_empty());

        let mut input = [2, 3].as_slice();
        assert_eq!(b.read_from(&mut input), Some(&mut [0, 1, 2, 3]));
        assert_eq!(b.len, 4);
        assert!(input.is_empty());
    }

    #[test]
    fn buffer_read_more_input() {
        let mut b: Buffer<[u8; 4]> = Buffer::default();

        let mut input = [0, 1, 2, 3, 4, 5].as_slice();
        assert_eq!(b.read_from(&mut input), Some(&mut [0, 1, 2, 3]));
        assert_eq!(b.len, 4);
        assert_eq!(input, [4, 5]);
    }
}
