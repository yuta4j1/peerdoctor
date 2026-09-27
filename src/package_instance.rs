use std::fmt;

use crate::version::{Version, VersionRange};

#[derive(Clone)]
pub struct PeerRequirement {
    pub(crate) package_name: String,
    pub(crate) range: VersionRange,
    optional: bool,
}

impl PeerRequirement {
    pub fn new(package_name: &str, range: VersionRange, optional: bool) -> Self {
        Self {
            package_name: package_name.to_string(),
            range,
            optional,
        }
    }
}

impl fmt::Display for PeerRequirement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.package_name, self.range.as_str())?;
        if self.optional {
            f.write_str(" (optional)")?;
        }
        Ok(())
    }
}

#[derive(Clone)]
pub struct PackageInstance {
    pub(crate) path: String,
    name: String,
    version: Version,
    pub(crate) peer_requirements: Vec<PeerRequirement>,
}

impl PackageInstance {
    pub fn new(
        path: &str,
        name: &str,
        version: Version,
        peer_requirements: Vec<PeerRequirement>,
    ) -> Self {
        Self {
            path: path.to_string(),
            name: name.to_string(),
            version,
            peer_requirements,
        }
    }
}

impl fmt::Display for PackageInstance {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}@{}", self.name, self.version)
    }
}
