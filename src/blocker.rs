use std::fmt;

use crate::candidates::Resolution;
use crate::package_instance::{PackageInstance, PeerRequirement};
use crate::registry::RegistryError;

pub struct Blocker {
    pub(crate) package: PackageInstance,
    pub(crate) requirement: PeerRequirement,
    pub(crate) suggestion: Option<Result<Resolution, RegistryError>>,
}

impl Blocker {
    pub fn suggestion(&self) -> Option<&Result<Resolution, RegistryError>> {
        self.suggestion.as_ref()
    }
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
