use std::cmp::Ordering;
use std::fmt;
use std::sync::LazyLock;

use regex::Regex;

use super::{regex, trim_whitespace, version_pattern};

const MAX_LENGTH: usize = 256;

// npm は数を倍精度浮動小数点数で扱うので、正確に表せる 2^53 - 1 までを上限にする
const MAX_NUMBER: u64 = 9_007_199_254_740_991;

static VERSION: LazyLock<Regex> = LazyLock::new(|| regex(&format!("^{}$", version_pattern())));

#[derive(Clone)]
pub struct Version {
    major: u64,
    minor: u64,
    patch: u64,
    prerelease: Vec<Identifier>,
}

impl Version {
    pub(crate) fn parse(input: &str) -> Result<Self, VersionParseError> {
        // npm は文字列の長さを UTF-16 のコード単位で数える
        if input.encode_utf16().count() > MAX_LENGTH {
            return Err(VersionParseError::new(
                input,
                &format!("longer than {MAX_LENGTH} characters"),
            ));
        }

        let captures = VERSION
            .captures(trim_whitespace(input))
            .ok_or_else(|| VersionParseError::new(input, "not a valid version"))?;

        let major = parse_number(input, &captures[1], "major")?;
        let minor = parse_number(input, &captures[2], "minor")?;
        let patch = parse_number(input, &captures[3], "patch")?;
        let prerelease = match captures.get(4) {
            Some(matched) => matched.as_str().split('.').map(Identifier::parse).collect(),
            None => Vec::new(),
        };

        Ok(Self {
            major,
            minor,
            patch,
            prerelease,
        })
    }

    pub(crate) fn is_prerelease(&self) -> bool {
        !self.prerelease.is_empty()
    }

    pub(super) fn has_same_core(&self, other: &Self) -> bool {
        (self.major, self.minor, self.patch) == (other.major, other.minor, other.patch)
    }
}

fn parse_number(input: &str, digits: &str, name: &str) -> Result<u64, VersionParseError> {
    digits
        .parse::<u64>()
        .ok()
        .filter(|number| *number <= MAX_NUMBER)
        .ok_or_else(|| {
            VersionParseError::new(
                input,
                &format!("{name} version is larger than {MAX_NUMBER}"),
            )
        })
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.major, self.minor, self.patch)
            .cmp(&(other.major, other.minor, other.patch))
            .then_with(|| compare_prerelease(&self.prerelease, &other.prerelease))
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for Version {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for Version {}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)?;
        for (index, identifier) in self.prerelease.iter().enumerate() {
            let separator = if index == 0 { "-" } else { "." };
            write!(f, "{separator}{identifier}")?;
        }
        Ok(())
    }
}

#[derive(Clone, PartialEq)]
enum Identifier {
    Numeric(u64),
    Text(String),
}

impl Identifier {
    fn parse(identifier: &str) -> Self {
        if identifier.bytes().all(|byte| byte.is_ascii_digit())
            && let Ok(number) = identifier.parse::<u64>()
            && number < MAX_NUMBER
        {
            return Self::Numeric(number);
        }
        Self::Text(identifier.to_string())
    }

    // npm は数字だけの識別子を、大きすぎて Numeric にならないものも含めて浮動小数点数として比べる
    fn as_number(&self) -> Option<f64> {
        match self {
            Self::Numeric(number) => Some(*number as f64),
            Self::Text(text) if text.bytes().all(|byte| byte.is_ascii_digit()) => text.parse().ok(),
            Self::Text(_) => None,
        }
    }
}

impl fmt::Display for Identifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Numeric(number) => write!(f, "{number}"),
            Self::Text(text) => f.write_str(text),
        }
    }
}

fn compare_prerelease(a: &[Identifier], b: &[Identifier]) -> Ordering {
    match (a.is_empty(), b.is_empty()) {
        (true, true) => return Ordering::Equal,
        (false, true) => return Ordering::Less,
        (true, false) => return Ordering::Greater,
        (false, false) => {}
    }

    // npm は、違う識別子を数値として比べて等しくなった場合も、そこで比較を終える
    for (x, y) in a.iter().zip(b) {
        if x != y {
            return compare_identifiers(x, y);
        }
    }
    a.len().cmp(&b.len())
}

fn compare_identifiers(a: &Identifier, b: &Identifier) -> Ordering {
    if let (Identifier::Numeric(x), Identifier::Numeric(y)) = (a, b) {
        return x.cmp(y);
    }
    match (a.as_number(), b.as_number()) {
        (Some(x), Some(y)) => x.total_cmp(&y),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => a.to_string().cmp(&b.to_string()),
    }
}

#[derive(Debug, thiserror::Error)]
#[error("invalid version {input:?}: {reason}")]
pub struct VersionParseError {
    input: String,
    reason: String,
}

impl VersionParseError {
    fn new(input: &str, reason: &str) -> Self {
        Self {
            input: input.to_string(),
            reason: reason.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::version::{assert_all_match, load_fixture};

    const VERSIONS: &str = include_str!("../../tests/fixtures/semver/versions.json");
    const COMPARE: &str = include_str!("../../tests/fixtures/semver/compare.json");

    #[test]
    fn parse_matches_npm() {
        let entries = load_fixture(VERSIONS);
        let mut failures = Vec::new();

        for entry in &entries {
            let input = entry["input"].as_str().expect("input は文字列");
            let expected = entry["normalized"].as_str();
            let actual = Version::parse(input)
                .ok()
                .map(|version| version.to_string());
            if actual.as_deref() != expected {
                failures.push(format!(
                    "{input:?}: npm = {expected:?}, peerdoctor = {actual:?}"
                ));
            }
        }

        assert_all_match(&failures, entries.len());
    }

    #[test]
    fn compare_matches_npm() {
        let entries = load_fixture(COMPARE);
        let mut failures = Vec::new();

        for entry in &entries {
            let a = entry["a"].as_str().expect("a は文字列");
            let b = entry["b"].as_str().expect("b は文字列");
            let expected = match entry["expected"].as_i64() {
                Some(-1) => Ordering::Less,
                Some(0) => Ordering::Equal,
                Some(1) => Ordering::Greater,
                other => panic!("expected は -1 / 0 / 1: {other:?}"),
            };
            let actual = Version::parse(a)
                .expect("テストデータの a は読めるバージョン")
                .cmp(&Version::parse(b).expect("テストデータの b は読めるバージョン"));
            if actual != expected {
                failures.push(format!(
                    "{a:?} vs {b:?}: npm = {expected:?}, peerdoctor = {actual:?}"
                ));
            }
        }

        assert_all_match(&failures, entries.len());
    }
}
