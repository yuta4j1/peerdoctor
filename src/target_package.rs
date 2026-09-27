use std::fmt;

use crate::version::Version;

pub struct TargetPackage {
    pub(crate) name: String,
    pub(crate) version: Version,
}

impl TargetPackage {
    pub fn new(name: &str, version: Version) -> Self {
        Self {
            name: name.to_string(),
            version,
        }
    }

    pub fn parse(spec: &str) -> Result<Self, TargetSpecError> {
        let (name, version) = spec
            .rsplit_once('@')
            .filter(|(name, _)| !name.is_empty())
            .ok_or_else(|| TargetSpecError::MissingVersion(spec.to_string()))?;
        let version = Version::parse(version)
            .map_err(|_| TargetSpecError::NotExactVersion(spec.to_string()))?;
        Ok(Self::new(name, version))
    }
}

impl fmt::Display for TargetPackage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}@{}", self.name, self.version)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum TargetSpecError {
    #[error("{0:?} has no version; specify the target as <name>@<version>, e.g. next@16.3.0")]
    MissingVersion(String),
    #[error("{0:?} is not an exact version; specify an exact version, e.g. next@16.3.0")]
    NotExactVersion(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_name_and_exact_version() {
        let target = TargetPackage::parse("next@16.3.0").expect("読める");
        assert_eq!(target.to_string(), "next@16.3.0");
    }

    #[test]
    fn reads_scoped_names() {
        let target = TargetPackage::parse("@acme/next-plugin@2.0.0").expect("読める");
        assert_eq!(target.name, "@acme/next-plugin");
        assert_eq!(target.version.to_string(), "2.0.0");
    }

    #[test]
    fn rejects_a_target_without_version() {
        for spec in ["next", "@acme/next-plugin", "next@"] {
            assert!(
                TargetPackage::parse(spec).is_err(),
                "{spec:?} は読めないはず"
            );
        }
        assert!(matches!(
            TargetPackage::parse("@acme/next-plugin"),
            Err(TargetSpecError::MissingVersion(_))
        ));
    }

    #[test]
    fn rejects_a_range_instead_of_an_exact_version() {
        for spec in ["next@16", "next@^16.3.0", "next@16.x", "next@latest"] {
            assert!(
                matches!(
                    TargetPackage::parse(spec),
                    Err(TargetSpecError::NotExactVersion(_))
                ),
                "{spec:?} は正確なバージョンではない"
            );
        }
    }
}
