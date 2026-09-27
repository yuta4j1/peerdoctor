mod lockfile;
mod manifest;

use std::path::Path;

pub use lockfile::{Lockfile, LockfileError, Unsupported};

pub struct Project {
    lockfile: Lockfile,
    overrides: Overrides,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Overrides {
    NotDeclared,
    Declared,
    Unknown,
}

impl Project {
    pub fn read(directory: &Path) -> Result<Self, LockfileError> {
        Ok(Self {
            lockfile: Lockfile::read(directory)?,
            overrides: manifest::read_overrides(directory),
        })
    }

    pub fn lockfile(&self) -> &Lockfile {
        &self.lockfile
    }

    pub fn overrides(&self) -> Overrides {
        self.overrides
    }
}
