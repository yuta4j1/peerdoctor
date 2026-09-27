use std::fs;
use std::path::PathBuf;

use super::{Packument, Registry, RegistryError};

pub(crate) struct FixtureRegistry {
    directory: PathBuf,
}

impl FixtureRegistry {
    pub(crate) fn new() -> Self {
        Self {
            directory: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/packuments"),
        }
    }
}

impl Registry for FixtureRegistry {
    fn packument(&self, name: &str) -> Result<Packument, RegistryError> {
        let json =
            fs::read_to_string(self.directory.join(format!("{name}.json"))).map_err(|_| {
                RegistryError::NotFound {
                    name: name.to_string(),
                }
            })?;
        Packument::parse(name, &json)
    }
}
