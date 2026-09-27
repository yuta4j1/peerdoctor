use std::fmt;

use crate::version::Version;

pub struct TargetPackage {
    pub(crate) name: String,
    pub(crate) version: Version,
}

impl TargetPackage {
    pub fn new(name: &str, version: Version) -> Self {
        Self {
            name: name.to_string(),
            version,
        }
    }
}

impl fmt::Display for TargetPackage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}@{}", self.name, self.version)
    }
}
