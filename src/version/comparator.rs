use std::cmp::Ordering;
use std::fmt;
use std::sync::LazyLock;

use regex::Regex;

use super::{OPERATOR, Version, collapse_whitespace, regex, version_pattern, whitespace_class};

static COMPARATOR: LazyLock<Regex> = LazyLock::new(|| {
    regex(&format!(
        "^{OPERATOR}{}?({})$|^$",
        whitespace_class(),
        version_pattern()
    ))
});

#[derive(Clone)]
pub(super) enum Comparator {
    Any,
    Constraint {
        operator: Operator,
        version: Version,
    },
}

#[derive(Clone)]
pub(super) enum Operator {
    Less,
    LessOrEqual,
    Greater,
    GreaterOrEqual,
    Equal,
}

pub(super) fn has_comparator_form(text: &str) -> bool {
    COMPARATOR.is_match(text)
}

impl Comparator {
    pub(super) fn parse(input: &str) -> Option<Self> {
        let normalized = collapse_whitespace(input);
        let captures = COMPARATOR.captures(&normalized)?;
        let Some(version) = captures.get(2) else {
            return Some(Self::Any);
        };
        let operator = match captures.get(1).map_or("", |matched| matched.as_str()) {
            "<" => Operator::Less,
            "<=" => Operator::LessOrEqual,
            ">" => Operator::Greater,
            ">=" => Operator::GreaterOrEqual,
            "" | "=" => Operator::Equal,
            other => unreachable!("演算子ではない: {other:?}"),
        };
        let version = Version::parse(version.as_str()).ok()?;
        Some(Self::Constraint { operator, version })
    }

    pub(super) fn matches(&self, version: &Version) -> bool {
        let Self::Constraint {
            operator,
            version: bound,
        } = self
        else {
            return true;
        };
        let ordering = version.cmp(bound);
        match operator {
            Operator::Less => ordering == Ordering::Less,
            Operator::LessOrEqual => ordering != Ordering::Greater,
            Operator::Greater => ordering == Ordering::Greater,
            Operator::GreaterOrEqual => ordering != Ordering::Less,
            Operator::Equal => ordering == Ordering::Equal,
        }
    }

    pub(super) fn is_any(&self) -> bool {
        matches!(self, Self::Any)
    }

    pub(super) fn is_null_set(&self) -> bool {
        self.to_string() == "<0.0.0-0"
    }
}

impl fmt::Display for Comparator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Any => Ok(()),
            Self::Constraint { operator, version } => write!(f, "{operator}{version}"),
        }
    }
}

impl fmt::Display for Operator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Less => "<",
            Self::LessOrEqual => "<=",
            Self::Greater => ">",
            Self::GreaterOrEqual => ">=",
            Self::Equal => "",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::version::{assert_all_match, load_fixture};

    const COMPARATORS: &str = include_str!("../../tests/fixtures/semver/comparators.json");

    #[test]
    fn comparator_matches_npm() {
        let entries = load_fixture(COMPARATORS);
        let mut failures = Vec::new();

        for entry in &entries {
            let input = entry["input"].as_str().expect("input は文字列");

            let comparator = Comparator::parse(input);
            let expected_value = entry["value"].as_str();
            let actual_value = comparator.as_ref().map(|comparator| comparator.to_string());
            if actual_value.as_deref() != expected_value {
                failures.push(format!(
                    "{input:?} の読み取り: npm = {expected_value:?}, peerdoctor = {actual_value:?}"
                ));
                continue;
            }

            let Some(comparator) = comparator else {
                continue;
            };
            for (key, expected) in [("matches", true), ("rejects", false)] {
                for version in entry[key].as_array().expect("matches / rejects は配列") {
                    let version = version.as_str().expect("バージョンは文字列");
                    let actual = comparator.matches(
                        &Version::parse(version).expect("テストデータのバージョンは読める"),
                    );
                    if actual != expected {
                        failures.push(format!(
                            "{input:?} を {version:?} が満たすか: npm = {expected}, peerdoctor = {actual}"
                        ));
                    }
                }
            }
        }

        assert_all_match(&failures, entries.len());
    }
}
