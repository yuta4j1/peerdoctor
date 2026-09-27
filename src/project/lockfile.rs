use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use serde::Deserialize;

use crate::package_instance::{PackageInstance, PeerRequirement};
use crate::version::{Version, VersionParseError};

pub struct Lockfile {
    packages: BTreeMap<String, PackageInstance>,
}

impl Lockfile {
    pub fn read(project: &Path) -> Result<Self, LockfileError> {
        let path = project.join("package-lock.json");
        let json = fs::read_to_string(&path).map_err(|error| LockfileError::Unreadable {
            path: path.display().to_string(),
            reason: error.to_string(),
        })?;
        Self::parse(&json)
    }

    pub fn parse(json: &str) -> Result<Self, LockfileError> {
        let raw: RawLockfile =
            serde_json::from_str(json).map_err(|error| LockfileError::InvalidJson {
                reason: error.to_string(),
            })?;
        if !matches!(raw.lockfile_version, 2 | 3) {
            return Err(LockfileError::UnsupportedVersion(raw.lockfile_version));
        }

        let raw_packages = raw.packages.ok_or(LockfileError::MissingPackages)?;
        let unsupported: Vec<Unsupported> = raw_packages
            .iter()
            .filter_map(|(path, raw_package)| {
                raw_package
                    .unsupported_reason(path)
                    .map(|reason| Unsupported {
                        path: path.clone(),
                        reason,
                    })
            })
            .collect();
        if !unsupported.is_empty() {
            return Err(LockfileError::Unsupported(unsupported));
        }

        let mut packages = BTreeMap::new();
        for (path, raw_package) in raw_packages {
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

    pub(crate) fn get(&self, path: &str) -> Option<&PackageInstance> {
        self.packages.get(path)
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
    name: Option<String>,
    version: Option<String>,
    resolved: Option<String>,
    #[serde(default)]
    link: bool,
    #[serde(default)]
    in_bundle: bool,
    bundle_dependencies: Option<serde_json::Value>,
    workspaces: Option<serde_json::Value>,
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
    fn unsupported_reason(&self, path: &str) -> Option<String> {
        let resolved = self.resolved.as_deref().unwrap_or("");
        if path.is_empty() {
            return self
                .workspaces
                .is_some()
                .then(|| "npm workspaces".to_string());
        }
        if self.link {
            return Some("linked package (a file: directory or a workspace member)".to_string());
        }
        if !path.starts_with("node_modules/") {
            return Some(
                "package outside node_modules (a file: directory or a workspace member)"
                    .to_string(),
            );
        }
        if resolved.starts_with("file:") {
            return Some("file: dependency".to_string());
        }
        if ["git+", "git:", "github:", "gitlab:", "bitbucket:"]
            .iter()
            .any(|prefix| resolved.starts_with(prefix))
        {
            return Some("git dependency".to_string());
        }
        if let Some(name) = &self.name
            && name != package_name(path)
        {
            return Some(format!(
                "npm alias ({} is installed under the name {})",
                name,
                package_name(path)
            ));
        }
        if self.in_bundle
            || self
                .bundle_dependencies
                .as_ref()
                .is_some_and(bundles_anything)
        {
            return Some("bundled dependencies".to_string());
        }
        None
    }

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
            .map(|(peer, spec)| {
                let optional = self
                    .peer_dependencies_meta
                    .get(peer)
                    .is_some_and(|meta| meta.optional);
                PeerRequirement::new(peer, spec, optional)
            })
            .collect();

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

fn bundles_anything(bundle_dependencies: &serde_json::Value) -> bool {
    match bundle_dependencies {
        serde_json::Value::Bool(bundles) => *bundles,
        serde_json::Value::Array(names) => !names.is_empty(),
        _ => false,
    }
}

#[derive(Debug)]
pub struct Unsupported {
    path: String,
    reason: String,
}

fn describe_unsupported(unsupported: &[Unsupported]) -> String {
    unsupported
        .iter()
        .map(|entry| {
            let place = if entry.path.is_empty() {
                "(project root)"
            } else {
                &entry.path
            };
            format!("\n  {place}: {}", entry.reason)
        })
        .collect()
}

#[derive(Debug, thiserror::Error)]
pub enum LockfileError {
    #[error("unsupported configuration:{}", describe_unsupported(.0))]
    Unsupported(Vec<Unsupported>),
    #[error("cannot read {path}: {reason}")]
    Unreadable { path: String, reason: String },
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
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASIC: &str = include_str!("../../tests/fixtures/lockfiles/basic/package-lock.json");
    const BASIC_V2: &str =
        include_str!("../../tests/fixtures/lockfiles/basic-v2/package-lock.json");
    const NESTED: &str = include_str!("../../tests/fixtures/lockfiles/nested/package-lock.json");
    const SCOPED: &str = include_str!("../../tests/fixtures/lockfiles/scoped/package-lock.json");
    const UNVERIFIED: &str =
        include_str!("../../tests/fixtures/lockfiles/unverified/package-lock.json");
    const OVERRIDES: &str =
        include_str!("../../tests/fixtures/lockfiles/overrides/package-lock.json");
    const WORKSPACES: &str =
        include_str!("../../tests/fixtures/lockfiles/unsupported-workspaces/package-lock.json");
    const LINK: &str =
        include_str!("../../tests/fixtures/lockfiles/unsupported-link/package-lock.json");
    const FILE: &str =
        include_str!("../../tests/fixtures/lockfiles/unsupported-file/package-lock.json");
    const ALIAS: &str =
        include_str!("../../tests/fixtures/lockfiles/unsupported-alias/package-lock.json");
    const BUNDLED: &str =
        include_str!("../../tests/fixtures/lockfiles/unsupported-bundled/package-lock.json");

    fn unsupported(json: &str) -> Vec<String> {
        match Lockfile::parse(json) {
            Err(LockfileError::Unsupported(entries)) => entries
                .iter()
                .map(|entry| format!("{}: {}", entry.path, entry.reason))
                .collect(),
            Err(other) => panic!("未対応の構成として拒否されるはず: {other}"),
            Ok(_) => panic!("未対応の構成として拒否されるはず"),
        }
    }

    #[test]
    fn rejects_workspaces() {
        assert_eq!(
            unsupported(WORKSPACES),
            [
                ": npm workspaces",
                "node_modules/member: linked package (a file: directory or a workspace member)",
                "packages/member: package outside node_modules (a file: directory or a workspace member)",
            ]
        );
    }

    #[test]
    fn rejects_linked_directories() {
        assert_eq!(
            unsupported(LINK),
            [
                "local-lib: package outside node_modules (a file: directory or a workspace member)",
                "node_modules/local-lib: linked package (a file: directory or a workspace member)",
            ]
        );
    }

    #[test]
    fn rejects_file_tarballs() {
        assert_eq!(
            unsupported(FILE),
            ["node_modules/local-tgz: file: dependency"]
        );
    }

    #[test]
    fn rejects_aliases() {
        assert_eq!(
            unsupported(ALIAS),
            [
                "node_modules/plugin-alias: npm alias (plugin-a is installed under the name plugin-alias)"
            ]
        );
    }

    #[test]
    fn rejects_bundled_dependencies() {
        assert_eq!(
            unsupported(BUNDLED),
            ["node_modules/bundle-host: bundled dependencies"]
        );
    }

    #[test]
    fn rejects_git_dependencies() {
        let json = r#"{
            "lockfileVersion": 3,
            "packages": {
                "": {},
                "node_modules/from-git": {
                    "version": "1.0.0",
                    "resolved": "git+ssh://git@github.com/acme/from-git.git#0123456789abcdef0123456789abcdef01234567"
                }
            }
        }"#;
        assert_eq!(unsupported(json), ["node_modules/from-git: git dependency"]);
    }

    #[test]
    fn accepts_overrides_because_the_lockfile_does_not_record_them() {
        assert!(Lockfile::parse(OVERRIDES).is_ok());
    }

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
                    "node_modules/broken": { "resolved": "https://registry.npmjs.org/broken/-/broken-1.0.0.tgz" }
                }
            }"#,
        );
        assert!(matches!(
            result,
            Err(LockfileError::MissingVersion { path }) if path == "node_modules/broken"
        ));
    }

    #[test]
    fn keeps_peer_specs_that_are_not_ranges() {
        assert!(
            summarize(UNVERIFIED)
                .contains(&"node_modules/plugin-tag plugin-tag@1.0.0 [next: latest]".to_string())
        );
    }
}
