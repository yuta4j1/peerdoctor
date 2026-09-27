#[cfg(test)]
pub(crate) mod fixture;
mod http;

pub use http::HttpRegistry;

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::package_instance::PeerRequirement;
use crate::version::Version;

pub trait Registry {
    fn packument(&self, name: &str) -> Result<Packument, RegistryError>;
}

#[derive(Clone)]
pub struct Packument {
    versions: Vec<PublishedVersion>,
    latest: Option<Version>,
}

#[derive(Clone)]
pub(crate) struct PublishedVersion {
    pub(crate) version: Version,
    pub(crate) peer_requirements: Vec<PeerRequirement>,
    pub(crate) deprecated: Option<String>,
}

impl Packument {
    pub(crate) fn parse(name: &str, json: &str) -> Result<Self, RegistryError> {
        let raw: RawPackument =
            serde_json::from_str(json).map_err(|error| RegistryError::InvalidResponse {
                name: name.to_string(),
                reason: error.to_string(),
            })?;

        // npm は semver として読めない版を範囲で選ぶことがないので、候補にも含めない
        let mut versions: Vec<PublishedVersion> = raw
            .versions
            .iter()
            .filter_map(|(version, raw_version)| {
                Some(PublishedVersion {
                    version: Version::parse(version).ok()?,
                    peer_requirements: raw_version.peer_requirements(),
                    deprecated: raw_version.deprecation_message(),
                })
            })
            .collect();
        versions.sort_by(|a, b| a.version.cmp(&b.version));

        let latest = raw
            .dist_tags
            .get("latest")
            .and_then(|latest| Version::parse(latest).ok());

        Ok(Self { versions, latest })
    }

    pub(crate) fn versions(&self) -> &[PublishedVersion] {
        &self.versions
    }

    pub(crate) fn latest(&self) -> Option<&Version> {
        self.latest.as_ref()
    }
}

#[derive(Deserialize)]
struct RawPackument {
    #[serde(rename = "dist-tags", default)]
    dist_tags: BTreeMap<String, String>,
    #[serde(default)]
    versions: BTreeMap<String, RawVersion>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawVersion {
    #[serde(default)]
    peer_dependencies: BTreeMap<String, String>,
    #[serde(default)]
    peer_dependencies_meta: BTreeMap<String, RawPeerDependencyMeta>,
    deprecated: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct RawPeerDependencyMeta {
    #[serde(default)]
    optional: bool,
}

impl RawVersion {
    fn peer_requirements(&self) -> Vec<PeerRequirement> {
        self.peer_dependencies
            .iter()
            .map(|(peer, spec)| {
                let optional = self
                    .peer_dependencies_meta
                    .get(peer)
                    .is_some_and(|meta| meta.optional);
                PeerRequirement::new(peer, spec, optional)
            })
            .collect()
    }

    // 空文字は非推奨の取り消しを表す
    fn deprecation_message(&self) -> Option<String> {
        match &self.deprecated {
            Some(serde_json::Value::String(message)) if !message.is_empty() => {
                Some(message.clone())
            }
            Some(serde_json::Value::Bool(true)) => Some(String::new()),
            _ => None,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RegistryError {
    #[error("{name} was not found in the registry")]
    NotFound { name: String },
    #[error("the registry refused access to {name} (authentication is not supported)")]
    Unauthorized { name: String },
    #[error("timed out while fetching {name}")]
    Timeout { name: String },
    #[error("could not reach the registry for {name}: {reason}")]
    Unreachable { name: String, reason: String },
    #[error("the registry returned an unexpected response for {name}: {reason}")]
    InvalidResponse { name: String, reason: String },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::fixture::FixtureRegistry;

    fn describe(packument: &Packument) -> Vec<String> {
        packument
            .versions()
            .iter()
            .map(|published| {
                let peers = published
                    .peer_requirements
                    .iter()
                    .map(|requirement| requirement.to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                let deprecated = if published.deprecated.is_some() {
                    " (deprecated)"
                } else {
                    ""
                };
                format!("{} [{peers}]{deprecated}", published.version)
            })
            .collect()
    }

    #[test]
    fn reads_every_version_in_order() {
        let packument = FixtureRegistry::new()
            .packument("plugin-a")
            .expect("テストデータにある");
        assert_eq!(
            describe(&packument),
            [
                "2.0.0 [next: ^14]",
                "2.1.0 [next: ^14 || ^15]",
                "2.9.0 [next: ^15]",
                "3.0.0 [next: ^16]",
                "3.0.5 [next: ^16] (deprecated)",
                "3.1.0 [next: ^16]",
                "3.2.1 [next: ^16]",
                "4.0.0-beta.1 [next: ^17]",
                "4.0.0 [next: ^17]",
                "4.1.0 [next: ^17]",
            ]
        );
        assert_eq!(
            packument
                .latest()
                .map(|latest| latest.to_string())
                .as_deref(),
            Some("4.1.0")
        );
    }

    #[test]
    fn reads_scoped_packages_and_versions_without_peers() {
        let packument = FixtureRegistry::new()
            .packument("@acme/next-plugin")
            .expect("テストデータにある");
        assert_eq!(
            describe(&packument),
            ["1.0.0 [next: ^15]", "2.0.0 [next: ^16]", "3.0.0 []"]
        );
    }

    #[test]
    fn reads_deprecation_messages() {
        let packument = Packument::parse(
            "example",
            r#"{
                "versions": {
                    "1.0.0": { "deprecated": "use 2.0.0" },
                    "1.1.0": { "deprecated": "" },
                    "1.2.0": { "deprecated": false },
                    "not-a-version": {}
                }
            }"#,
        )
        .expect("読める");
        assert_eq!(
            describe(&packument),
            ["1.0.0 [] (deprecated)", "1.1.0 []", "1.2.0 []"]
        );
        assert!(packument.latest().is_none());
    }

    #[test]
    fn reports_missing_packages() {
        let result = FixtureRegistry::new().packument("left-pad");
        assert!(matches!(result, Err(RegistryError::NotFound { name }) if name == "left-pad"));
    }
}
