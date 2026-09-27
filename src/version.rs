pub struct Version(nodejs_semver::Version);

impl Version {
    pub(crate) fn parse(input: &str) -> Result<Self, VersionParseError> {
        let version = nodejs_semver::Version::parse(input)
            .map_err(|error| VersionParseError::new(input, error))?;
        Ok(Self(version))
    }
}

#[derive(Debug, thiserror::Error)]
#[error("invalid version {input:?}: {reason}")]
pub struct VersionParseError {
    input: String,
    reason: String,
}

impl VersionParseError {
    fn new(input: &str, error: nodejs_semver::SemverError) -> Self {
        Self {
            input: input.to_string(),
            reason: error.to_string(),
        }
    }
}

#[derive(Clone)]
pub struct VersionRange(nodejs_semver::Range);

impl VersionRange {
    pub(crate) fn parse(input: &str) -> Result<Self, VersionRangeParseError> {
        // npm（JS の semver）は範囲を || で区切り、各部分を個別に読む。
        // - 不正な部分が1つでもあれば、範囲全体がエラー
        // - 空の部分が1つでもあれば、範囲全体が * と同じ
        // nodejs-semver クレートは不正な部分や、一部だけ空の部分を黙って捨てるので、
        // ここで JS 版に合わせる。
        let parts: Vec<&str> = input.split("||").map(|part| part.trim()).collect();

        for part in parts.iter().filter(|part| !part.is_empty()) {
            nodejs_semver::Range::parse(part)
                .map_err(|error| VersionRangeParseError::new(input, error))?;
        }

        if parts.iter().any(|part| part.is_empty()) {
            return Ok(Self(nodejs_semver::Range::any()));
        }

        let range = nodejs_semver::Range::parse(input)
            .map_err(|error| VersionRangeParseError::new(input, error))?;
        Ok(Self(range))
    }

    pub(crate) fn satisfies(&self, version: Version) -> bool {
        todo!("npm 互換の範囲判定。semver クレートを選定してから実装する")
    }
}

#[derive(Debug, thiserror::Error)]
#[error("invalid version range {input:?}: {reason}")]
pub struct VersionRangeParseError {
    input: String,
    reason: String,
}

impl VersionRangeParseError {
    fn new(input: &str, error: nodejs_semver::SemverError) -> Self {
        Self {
            input: input.to_string(),
            reason: error.to_string(),
        }
    }
}
