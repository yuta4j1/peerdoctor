use std::fmt;

use crate::version::{Version, VersionRange};

#[derive(Clone)]
pub struct PeerRequirement {
    pub(crate) package_name: String,
    pub(crate) spec: PeerSpec,
    optional: bool,
}

#[derive(Clone)]
pub enum PeerSpec {
    Range(VersionRange),
    NotARange(String),
}

impl PeerRequirement {
    pub fn new(package_name: &str, spec: &str, optional: bool) -> Self {
        let spec = match VersionRange::parse(spec) {
            Ok(range) => PeerSpec::Range(range),
            Err(_) => PeerSpec::NotARange(spec.to_string()),
        };
        Self {
            package_name: package_name.to_string(),
            spec,
            optional,
        }
    }
}

impl fmt::Display for PeerRequirement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let spec = match &self.spec {
            PeerSpec::Range(range) => range.as_str(),
            PeerSpec::NotARange(text) => text,
        };
        write!(f, "{}: {spec}", self.package_name)?;
        if self.optional {
            f.write_str(" (optional)")?;
        }
        Ok(())
    }
}

#[derive(Clone)]
pub struct PackageInstance {
    pub(crate) path: String,
    pub(crate) name: String,
    pub(crate) version: Version,
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
