use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use serde::Deserialize;

use super::Overrides;

#[derive(Deserialize)]
struct RawManifest {
    #[serde(default)]
    overrides: BTreeMap<String, serde_json::Value>,
}

pub(super) fn read_overrides(project: &Path) -> Overrides {
    match fs::read_to_string(project.join("package.json")) {
        Ok(json) => overrides_in(&json),
        Err(_) => Overrides::Unknown,
    }
}

fn overrides_in(json: &str) -> Overrides {
    match serde_json::from_str::<RawManifest>(json) {
        Ok(manifest) if manifest.overrides.is_empty() => Overrides::NotDeclared,
        Ok(_) => Overrides::Declared,
        Err(_) => Overrides::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASIC: &str = include_str!("../../tests/fixtures/lockfiles/basic/package.json");
    const OVERRIDES: &str = include_str!("../../tests/fixtures/lockfiles/overrides/package.json");

    #[test]
    fn finds_declared_overrides() {
        assert_eq!(overrides_in(OVERRIDES), Overrides::Declared);
        assert_eq!(overrides_in(BASIC), Overrides::NotDeclared);
        assert_eq!(
            overrides_in(r#"{ "overrides": {} }"#),
            Overrides::NotDeclared
        );
    }

    #[test]
    fn cannot_tell_when_package_json_is_broken() {
        assert_eq!(overrides_in("not json"), Overrides::Unknown);
        assert_eq!(
            read_overrides(Path::new("/nonexistent")),
            Overrides::Unknown
        );
    }
}
