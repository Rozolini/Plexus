use std::collections::HashMap;
use std::hash::Hash;
use std::sync::{Arc, Mutex};

use thiserror::Error;

use crate::arena::{ArenaError, SharedArena};
use crate::ptr::Handle;

#[derive(Debug, Error)]
pub enum PlexusMapError {
    #[error("arena error: {0}")]
    Arena(#[from] ArenaError),
    #[error("internal synchronization poisoned")]
    Poisoned,
}

/// IPC-safe map that stores values by arena handle.
pub struct PlexusMap<K, V>
where
    K: Copy + Eq + Hash,
    V: Copy,
{
    arena: Arc<Mutex<SharedArena>>,
    entries: HashMap<K, Handle<V>>,
}

impl<K, V> PlexusMap<K, V>
where
    K: Copy + Eq + Hash,
    V: Copy,
{
    #[must_use]
    pub fn new(arena: Arc<Mutex<SharedArena>>) -> Self {
        Self {
            arena,
            entries: HashMap::new(),
        }
    }

    pub fn insert(&mut self, key: K, value: V) -> Result<(), PlexusMapError> {
        let mut arena = self.arena.lock().map_err(|_| PlexusMapError::Poisoned)?;
        arena.begin_publish();
        let handle = arena.allocate_value(value)?;
        arena.commit();
        self.entries.insert(key, handle);
        Ok(())
    }

    pub fn get(&self, key: K) -> Result<Option<V>, PlexusMapError> {
        let Some(handle) = self.entries.get(&key).copied() else {
            return Ok(None);
        };
        let arena = self.arena.lock().map_err(|_| PlexusMapError::Poisoned)?;
        Ok(Some(arena.read_copy(handle)?))
    }
}
