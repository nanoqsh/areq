#[derive(Default)]
pub(crate) struct Buffer<B> {
    data: B,
    len: usize,
}

impl<B> Buffer<B>
where
    B: AsMut<[u8]>,
{
    pub fn read_from(&mut self, input: &mut &[u8]) -> Option<&mut B> {
        let write = self.get_mut();
        let n = usize::min(input.len(), write.len());
        let (left, right) = input.split_at(n);
        write[..n].copy_from_slice(left);
        *input = right;
        self.len += n;

        self.filled()
    }

    fn filled(&mut self) -> Option<&mut B> {
        if self.get_mut().is_empty() {
            Some(&mut self.data)
        } else {
            None
        }
    }

    fn get_mut(&mut self) -> &mut [u8] {
        &mut self.data.as_mut()[..self.len]
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
