use core::marker::PhantomData;
use core::mem::size_of;
use core::ptr::NonNull;

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RelPtr<T> {
    offset: i64,
    _marker: PhantomData<fn() -> T>,
}

impl<T> RelPtr<T> {
    #[must_use]
    pub const fn null() -> Self {
        Self {
            offset: 0,
            _marker: PhantomData,
        }
    }

    #[must_use]
    pub const fn is_null(&self) -> bool {
        self.offset == 0
    }

    /// # Safety
    /// `target` must be in the same mapped region and live long enough.
    /// Invariant: `INV-REL-PTR`.
    pub unsafe fn from_ptr(this: NonNull<Self>, target: NonNull<T>) -> Self {
        let base = this.as_ptr() as isize;
        let target = target.as_ptr() as isize;
        let delta = target.wrapping_sub(base);
        Self {
            offset: delta as i64,
            _marker: PhantomData,
        }
    }

    pub fn resolve(&self, this: NonNull<Self>) -> Option<NonNull<T>> {
        if self.is_null() {
            return None;
        }

        let this_addr = this.as_ptr() as isize;
        let target_addr = this_addr.wrapping_add(self.offset as isize);
        NonNull::new(target_addr as *mut T)
    }

    #[must_use]
    pub const fn raw_offset(&self) -> i64 {
        self.offset
    }

    #[must_use]
    pub const fn byte_size() -> usize {
        size_of::<Self>()
    }
}
