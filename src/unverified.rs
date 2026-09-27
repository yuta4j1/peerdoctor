use std::fmt;

use crate::package_instance::{PackageInstance, PeerRequirement};

pub struct Unverified {
    pub(crate) package: PackageInstance,
    pub(crate) requirement: PeerRequirement,
    pub(crate) reason: UnverifiedReason,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnverifiedReason {
    NotARange,
}

impl Unverified {
    pub fn reason(&self) -> UnverifiedReason {
        self.reason
    }
}

impl fmt::Display for Unverified {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} ({})  peer {}  ({})",
            self.package, self.package.path, self.requirement, self.reason
        )
    }
}

impl fmt::Display for UnverifiedReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::NotARange => {
                "not a version range; npm treats it as a dist-tag, which peerdoctor cannot check"
            }
        })
    }
}
