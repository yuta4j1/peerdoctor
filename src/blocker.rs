use crate::package_instance::{PackageInstance, PeerRequirement};

pub struct Blocker {
    pub(crate) package: PackageInstance,
    pub(crate) requirement: PeerRequirement,
}
