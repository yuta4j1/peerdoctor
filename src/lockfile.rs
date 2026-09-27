use std::collections::BTreeMap;

use serde::Deserialize;

use crate::package_instance::{PackageInstance, PeerRequirement};
use crate::version::{Version, VersionParseError, VersionRange, VersionRangeParseError};

pub struct Lockfile {
    packages: BTreeMap<String, PackageInstance>,
}

impl Lockfile {
    pub fn parse(json: &str) -> Result<Self, LockfileError> {
        let raw: RawLockfile =
            serde_json::from_str(json).map_err(|error| LockfileError::InvalidJson {
                reason: error.to_string(),
            })?;
        if !matches!(raw.lockfile_version, 2 | 3) {
            return Err(LockfileError::UnsupportedVersion(raw.lockfile_version));
        }

        let mut packages = BTreeMap::new();
        for (path, raw_package) in raw.packages.ok_or(LockfileError::MissingPackages)? {
            // 空のパスはプロジェクト自身で、依存ではない
            if path.is_empty() {
                continue;
            }
            let package = raw_package.to_instance(&path)?;
            packages.insert(path, package);
        }
        Ok(Self { packages })
    }

    pub fn packages(&self) -> impl Iterator<Item = &PackageInstance> {
        self.packages.values()
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawLockfile {
    lockfile_version: u64,
    packages: Option<BTreeMap<String, RawPackage>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawPackage {
    version: Option<String>,
    #[serde(default)]
    peer_dependencies: BTreeMap<String, String>,
    #[serde(default)]
    peer_dependencies_meta: BTreeMap<String, RawPeerDependencyMeta>,
}

#[derive(Deserialize)]
struct RawPeerDependencyMeta {
    #[serde(default)]
    optional: bool,
}

impl RawPackage {
    fn to_instance(&self, path: &str) -> Result<PackageInstance, LockfileError> {
        let version = self
            .version
            .as_deref()
            .ok_or_else(|| LockfileError::MissingVersion {
                path: path.to_string(),
            })?;
        let version = Version::parse(version).map_err(|source| LockfileError::InvalidVersion {
            path: path.to_string(),
            source,
        })?;

        let peer_requirements = self
            .peer_dependencies
            .iter()
            .map(|(peer, range)| {
                let range = VersionRange::parse(range).map_err(|source| {
                    LockfileError::InvalidPeerRange {
                        path: path.to_string(),
                        peer: peer.clone(),
                        source,
                    }
                })?;
                let optional = self
                    .peer_dependencies_meta
                    .get(peer)
                    .is_some_and(|meta| meta.optional);
                Ok(PeerRequirement::new(peer, range, optional))
            })
            .collect::<Result<Vec<_>, LockfileError>>()?;

        Ok(PackageInstance::new(
            path,
            package_name(path),
            version,
            peer_requirements,
        ))
    }
}

fn package_name(path: &str) -> &str {
    path.rsplit_once("node_modules/")
        .map_or(path, |(_, name)| name)
}

#[derive(Debug, thiserror::Error)]
pub enum LockfileError {
    #[error("package-lock.json is not valid JSON: {reason}")]
    InvalidJson { reason: String },
    #[error("lockfileVersion {0} is not supported (supported: 2, 3)")]
    UnsupportedVersion(u64),
    #[error("package-lock.json has no \"packages\"")]
    MissingPackages,
    #[error("{path}: no version")]
    MissingVersion { path: String },
    #[error("{path}: {source}")]
    InvalidVersion {
        path: String,
        source: VersionParseError,
    },
    #[error("{path}: peer {peer:?}: {source}")]
    InvalidPeerRange {
        path: String,
        peer: String,
        source: VersionRangeParseError,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASIC: &str = include_str!("../tests/fixtures/lockfiles/basic/package-lock.json");
    const BASIC_V2: &str = include_str!("../tests/fixtures/lockfiles/basic-v2/package-lock.json");
    const NESTED: &str = include_str!("../tests/fixtures/lockfiles/nested/package-lock.json");
    const SCOPED: &str = include_str!("../tests/fixtures/lockfiles/scoped/package-lock.json");

    fn summarize(json: &str) -> Vec<String> {
        Lockfile::parse(json)
            .expect("テストデータのロックファイルは読める")
            .packages()
            .map(|package| {
                let peers = package
                    .peer_requirements
                    .iter()
                    .map(|requirement| requirement.to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{} {package} [{peers}]", package.path)
            })
            .collect()
    }

    #[test]
    fn reads_packages_with_their_placement() {
        assert_eq!(
            summarize(BASIC),
            [
                "node_modules/next next@15.3.0 [react: ^18 || ^19]",
                "node_modules/plugin-a plugin-a@2.1.0 [next: ^14 || ^15]",
                "node_modules/plugin-b plugin-b@3.0.0 [next: >=15 <17]",
                "node_modules/plugin-c plugin-c@1.0.0 [next: ^15, react: ^18 (optional)]",
                "node_modules/react react@18.2.0 []",
            ]
        );
    }

    #[test]
    fn reads_lockfile_v2_the_same_as_v3() {
        assert_eq!(summarize(BASIC_V2), summarize(BASIC));
    }

    #[test]
    fn keeps_nested_packages_with_the_same_name_apart() {
        assert_eq!(
            summarize(NESTED),
            [
                "node_modules/legacy-host legacy-host@1.0.0 []",
                "node_modules/legacy-host/node_modules/next next@14.2.0 [react: ^18]",
                "node_modules/legacy-host/node_modules/plugin-old plugin-old@1.0.0 [next: ^14]",
                "node_modules/next next@15.3.0 [react: ^18 || ^19]",
                "node_modules/plugin-a plugin-a@2.1.0 [next: ^14 || ^15]",
                "node_modules/react react@18.2.0 []",
            ]
        );
    }

    #[test]
    fn names_scoped_packages_by_two_path_segments() {
        assert_eq!(
            summarize(SCOPED),
            [
                "node_modules/@acme/next-plugin @acme/next-plugin@1.0.0 [next: ^15]",
                "node_modules/next next@15.3.0 [react: ^18 || ^19]",
                "node_modules/react react@18.2.0 []",
            ]
        );
    }

    #[test]
    fn rejects_lockfile_v1() {
        let result = Lockfile::parse(r#"{ "lockfileVersion": 1, "dependencies": {} }"#);
        assert!(matches!(result, Err(LockfileError::UnsupportedVersion(1))));
    }

    #[test]
    fn rejects_text_that_is_not_json() {
        let result = Lockfile::parse("not json");
        assert!(matches!(result, Err(LockfileError::InvalidJson { .. })));
    }

    #[test]
    fn rejects_a_package_without_version() {
        let result = Lockfile::parse(
            r#"{
                "lockfileVersion": 3,
                "packages": {
                    "": {},
                    "node_modules/linked": { "resolved": "packages/linked", "link": true }
                }
            }"#,
        );
        assert!(matches!(
            result,
            Err(LockfileError::MissingVersion { path }) if path == "node_modules/linked"
        ));
    }

    #[test]
    fn rejects_a_peer_range_that_cannot_be_parsed() {
        let result = Lockfile::parse(
            r#"{
                "lockfileVersion": 3,
                "packages": {
                    "": {},
                    "node_modules/plugin": {
                        "version": "1.0.0",
                        "peerDependencies": { "next": "latest" }
                    }
                }
            }"#,
        );
        assert!(matches!(
            result,
            Err(LockfileError::InvalidPeerRange { path, peer, .. })
                if path == "node_modules/plugin" && peer == "next"
        ));
    }
}
