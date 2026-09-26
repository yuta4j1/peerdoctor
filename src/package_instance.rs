use crate::version::{Version, VersionRange};

#[derive(Clone)]
pub struct PeerRequirement {
    pub(crate) package_name: String,
    pub(crate) range: VersionRange,
    optional: bool,
}

pub struct PackageInstance {
    name: String,
    pub(crate) peer_requirements: Vec<PeerRequirement>,
    version: Version,
}
