use std::sync::{Arc, Mutex};

use thiserror::Error;

use crate::arena::SharedArena;
use crate::containers::plexus_vec::{PlexusVec, PlexusVecError};

#[derive(Debug, Error)]
pub enum PlexusStringError {
    #[error("vector error: {0}")]
    Vec(#[from] PlexusVecError),
    #[error("invalid utf-8 in shared content")]
    InvalidUtf8,
}

pub struct PlexusString {
    bytes: PlexusVec<u8>,
}

impl PlexusString {
    #[must_use]
    pub fn new(arena: Arc<Mutex<SharedArena>>) -> Self {
        Self {
            bytes: PlexusVec::new(arena),
        }
    }

    pub fn push_str(&mut self, value: &str) -> Result<(), PlexusStringError> {
        for byte in value.bytes() {
            self.bytes.push(byte)?;
        }
        Ok(())
    }

    pub fn as_string(&self) -> Result<String, PlexusStringError> {
        let mut raw = Vec::with_capacity(self.bytes.len());
        for idx in 0..self.bytes.len() {
            if let Some(byte) = self.bytes.get(idx)? {
                raw.push(byte);
            }
        }
        String::from_utf8(raw).map_err(|_| PlexusStringError::InvalidUtf8)
    }
}
