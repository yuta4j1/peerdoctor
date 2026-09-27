use std::fmt;

use crate::package_instance::{PackageInstance, PeerRequirement};

pub struct Blocker {
    pub(crate) package: PackageInstance,
    pub(crate) requirement: PeerRequirement,
}

impl fmt::Display for Blocker {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} ({})  peer {}",
            self.package, self.package.path, self.requirement
        )
    }
}
