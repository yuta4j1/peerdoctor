use std::fmt;

use crate::blocker::Blocker;
use crate::candidates::{self, Resolution};
use crate::package_instance::PeerSpec;
use crate::project::Lockfile;
use crate::registry::Registry;
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

    pub fn suggest(&mut self, registry: &dyn Registry) {
        for blocker in &mut self.blockers {
            let resolution = registry
                .packument(&blocker.package.name)
                .map(|packument| candidates::resolve(&packument, &self.target_name, &self.to));
            blocker.suggestion = Some(resolution);
        }
    }

    fn unverified_count(&self) -> usize {
        let unfetched = self
            .blockers
            .iter()
            .filter(|blocker| matches!(blocker.suggestion, Some(Err(_))))
            .count();
        self.unverified.len() + unfetched
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
                            suggestion: None,
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
            match &blocker.suggestion {
                None => {}
                Some(Ok(Resolution::Satisfiable(candidates))) => {
                    writeln!(f, "    SATISFIED BY  {}", candidates.range)?;
                    write!(
                        f,
                        "                  minimum {} / newest in range {}",
                        candidates.minimum, candidates.newest
                    )?;
                    if let Some(latest) = &candidates.latest {
                        write!(f, " / latest overall {latest}")?;
                    }
                    writeln!(f)?;
                    for note in &candidates.notes {
                        writeln!(f, "                  note: {note}")?;
                    }
                }
                Some(Ok(Resolution::DeadEnd { latest })) => {
                    write!(
                        f,
                        "    DEAD END      no published version accepts {}@{}",
                        self.target_name, self.to
                    )?;
                    if let Some(latest) = latest {
                        write!(f, " (latest {latest})")?;
                    }
                    writeln!(f)?;
                }
                Some(Err(error)) => {
                    writeln!(
                        f,
                        "    UNVERIFIED    could not check other versions: {error}"
                    )?;
                }
            }
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
        write!(f, "Summary: {} {noun}", self.blockers.len())?;
        if self
            .blockers
            .iter()
            .any(|blocker| blocker.suggestion.is_some())
        {
            let satisfiable = self
                .blockers
                .iter()
                .filter(|blocker| {
                    matches!(blocker.suggestion, Some(Ok(Resolution::Satisfiable(_))))
                })
                .count();
            let dead_ends = self
                .blockers
                .iter()
                .filter(|blocker| {
                    matches!(blocker.suggestion, Some(Ok(Resolution::DeadEnd { .. })))
                })
                .count();
            write!(f, " ({satisfiable} satisfiable, {dead_ends} dead end)")?;
        }
        writeln!(f, ", {} unverified", self.unverified_count())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CheckError {
    #[error(
        "{0} is not installed at the project root (node_modules/{0}); adding a new package is not supported yet"
    )]
    TargetNotInstalled(String),
}
