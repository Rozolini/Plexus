use core::marker::PhantomData;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Handle<T> {
    offset: u64,
    generation: u64,
    _marker: PhantomData<fn() -> T>,
}

impl<T> Handle<T> {
    #[must_use]
    pub const fn from_parts(offset: u64, generation: u64) -> Self {
        Self {
            offset,
            generation,
            _marker: PhantomData,
        }
    }

    #[must_use]
    pub const fn from_offset(offset: u64) -> Self {
        Self::from_parts(offset, 1)
    }

    #[must_use]
    pub const fn offset(&self) -> u64 {
        self.offset
    }

    #[must_use]
    pub const fn generation(&self) -> u64 {
        self.generation
    }

    #[must_use]
    pub const fn cast<U>(&self) -> Handle<U> {
        Handle::<U>::from_parts(self.offset, self.generation)
    }
}
