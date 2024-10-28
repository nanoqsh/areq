use {
    bytes::BytesMut,
    std::{mem::MaybeUninit, slice},
};

/// A wrapper around [`BytesMut`] that ensures its spare capacity is initialized.
///
/// This type provides utility methods to work with [`BytesMut`], while maintaining
/// the invariant that its spare capacity is always initialized (zeroed).
/// This can be useful to use it with [`AsyncRead`](futures_io::AsyncRead)
/// that requires the buffer to be initialized, until this trait supports the
/// [`poll_read_buf`](https://github.com/rust-lang/futures-rs/issues/2209) method.
///
/// This way you can safely get an "uninitialized" part of the buffer via
/// [`spare_capacity_mut`](InitBytesMut::spare_capacity_mut) method that returns
/// a `&mut` slice of an actually initialized memory.
pub(crate) struct InitBytesMut(BytesMut);

impl InitBytesMut {
    pub fn new() -> Self {
        Self(BytesMut::new())
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn as_mut(&mut self) -> &mut [u8] {
        &mut self.0
    }

    pub fn split_to(&mut self, at: usize) -> BytesMut {
        self.0.split_to(at)
    }

    pub fn reserve(&mut self, n: usize) {
        self.0.reserve(n);

        // fill spare capacity with zeroes
        self.0.spare_capacity_mut().fill(MaybeUninit::zeroed());
    }

    pub fn spare_capacity_len(&self) -> usize {
        self.0.capacity() - self.0.len()
    }

    pub fn spare_capacity_mut(&mut self) -> &mut [u8] {
        let slice = self.0.spare_capacity_mut();

        // SAFETY: the type invariant is the spare capacity is always initializated
        unsafe { slice::from_raw_parts_mut(slice.as_mut_ptr().cast(), slice.len()) }
    }

    pub fn advance(&mut self, n: usize) {
        assert!(
            n <= self.spare_capacity_len(),
            "requires enough space to advance",
        );

        // SAFETY:
        // * capacity is checked
        // * all data are initializated
        unsafe { self.0.set_len(self.0.len() + n) };
    }
}
