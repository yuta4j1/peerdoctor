use std::fmt;

use crate::blocker::Blocker;
use crate::package_instance::PeerSpec;
use crate::project::Lockfile;
use crate::resolver::resolve_peer;
use crate::target_package::TargetPackage;
use crate::unverified::{Unverified, UnverifiedReason};
use crate::version::Version;

pub struct Report {
    target_name: String,
    from: Version,
    to: Version,
    blockers: Vec<Blocker>,
    unverified: Vec<Unverified>,
}

impl Report {
    pub fn blockers(&self) -> &[Blocker] {
        &self.blockers
    }

    pub fn unverified(&self) -> &[Unverified] {
        &self.unverified
    }
}

pub fn check(lockfile: &Lockfile, target: &TargetPackage) -> Result<Report, CheckError> {
    let installed = lockfile
        .get(&format!("node_modules/{}", target.name))
        .ok_or_else(|| CheckError::TargetNotInstalled(target.name.clone()))?;

    let mut blockers = Vec::new();
    let mut unverified = Vec::new();
    for package in lockfile.packages() {
        for requirement in &package.peer_requirements {
            if requirement.package_name != target.name {
                continue;
            }
            // ネストした同名パッケージなど、ルートのターゲット以外に解決される peer は変更の影響を受けない
            let resolves_to_target = resolve_peer(lockfile, package, &requirement.package_name)
                .is_some_and(|provider| provider.path == installed.path);
            if !resolves_to_target {
                continue;
            }
            match &requirement.spec {
                PeerSpec::Range(range) => {
                    if !range.satisfies(&target.version) {
                        blockers.push(Blocker {
                            package: package.clone(),
                            requirement: requirement.clone(),
                        });
                    }
                }
                PeerSpec::NotARange(_) => unverified.push(Unverified {
                    package: package.clone(),
                    requirement: requirement.clone(),
                    reason: UnverifiedReason::NotARange,
                }),
            }
        }
    }

    Ok(Report {
        target_name: target.name.clone(),
        from: installed.version.clone(),
        to: target.version.clone(),
        blockers,
        unverified,
    })
}

impl fmt::Display for Report {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "Target: {} {} -> {}",
            self.target_name, self.from, self.to
        )?;
        writeln!(f)?;
        writeln!(f, "BLOCKERS ({})", self.blockers.len())?;
        for blocker in &self.blockers {
            writeln!(f, "  {blocker}")?;
        }
        if !self.unverified.is_empty() {
            writeln!(f)?;
            writeln!(f, "UNVERIFIED ({})", self.unverified.len())?;
            for unverified in &self.unverified {
                writeln!(f, "  {unverified}")?;
            }
        }
        writeln!(f)?;
        let noun = if self.blockers.len() == 1 {
            "blocker"
        } else {
            "blockers"
        };
        writeln!(
            f,
            "Summary: {} {noun}, {} unverified",
            self.blockers.len(),
            self.unverified.len()
        )
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CheckError {
    #[error(
        "{0} is not installed at the project root (node_modules/{0}); adding a new package is not supported yet"
    )]
    TargetNotInstalled(String),
}
