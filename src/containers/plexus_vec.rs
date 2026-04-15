use std::alloc::Layout;
use std::sync::{Arc, Mutex};

use thiserror::Error;

use crate::arena::{ArenaError, SharedArena};
use crate::ptr::Handle;

#[derive(Debug, Error)]
pub enum PlexusVecError {
    #[error("arena error: {0}")]
    Arena(#[from] ArenaError),
    #[error("internal synchronization poisoned")]
    Poisoned,
}

/// IPC-safe vector storing values as arena handles to avoid absolute pointers.
pub struct PlexusVec<T>
where
    T: Copy,
{
    arena: Arc<Mutex<SharedArena>>,
    handles: Vec<Handle<T>>,
}

impl<T> PlexusVec<T>
where
    T: Copy,
{
    #[must_use]
    pub fn new(arena: Arc<Mutex<SharedArena>>) -> Self {
        Self {
            arena,
            handles: Vec::new(),
        }
    }

    pub fn push(&mut self, value: T) -> Result<(), PlexusVecError> {
        let mut arena = self.arena.lock().map_err(|_| PlexusVecError::Poisoned)?;
        arena.begin_publish();
        let handle = arena.allocate_value(value)?;
        arena.commit();
        self.handles.push(handle);
        Ok(())
    }

    pub fn get(&self, index: usize) -> Result<Option<T>, PlexusVecError> {
        let Some(handle) = self.handles.get(index).copied() else {
            return Ok(None);
        };
        let arena = self.arena.lock().map_err(|_| PlexusVecError::Poisoned)?;
        let value = arena.read_copy(handle)?;
        Ok(Some(value))
    }

    pub fn pop(&mut self) -> Result<Option<T>, PlexusVecError> {
        let Some(handle) = self.handles.pop() else {
            return Ok(None);
        };
        let arena = self.arena.lock().map_err(|_| PlexusVecError::Poisoned)?;
        let value = arena.read_copy(handle)?;
        arena.deallocate_layout(handle, Layout::new::<T>())?;
        Ok(Some(value))
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.handles.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.handles.is_empty()
    }
}
