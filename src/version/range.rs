use std::fmt;
use std::sync::LazyLock;

use regex::{Captures, Regex};

use super::comparator::{Comparator, has_comparator_form};
use super::{
    NUMBER, OPERATOR, Version, WHITESPACE, WORD, build_pattern, collapse_whitespace,
    prerelease_pattern, regex, trim_whitespace, version_pattern, whitespace_class,
};

const STRICT_NUMBER: &str = "0|[1-9][0-9]*";

fn partial_version_pattern() -> String {
    let part = format!(r"{NUMBER}|x|X|\*");
    format!(
        r"[v={WHITESPACE}]*({part})(?:\.({part})(?:\.({part})(?:{})?{}?)?)?",
        prerelease_pattern(),
        build_pattern()
    )
}

fn strict_partial_version_pattern() -> String {
    let part = format!(r"{STRICT_NUMBER}|x|X|\*");
    let identifier = format!("(?:{WORD}|{STRICT_NUMBER})");
    format!(
        r"[v={WHITESPACE}]*({part})(?:\.({part})(?:\.({part})(?:-({identifier}(?:\.{identifier})*))?{}?)?)?",
        build_pattern()
    )
}

static WHITESPACE_RUN: LazyLock<Regex> =
    LazyLock::new(|| regex(&format!("{}+", whitespace_class())));

static BUILD_METADATA: LazyLock<Regex> = LazyLock::new(|| regex(&build_pattern()));

static HYPHEN_RANGE: LazyLock<Regex> = LazyLock::new(|| {
    let (ws, partial) = (whitespace_class(), partial_version_pattern());
    regex(&format!("^{ws}?({partial}){ws}-{ws}({partial}){ws}?$"))
});

// npm と同じ位置で空白を詰めるため、厳密な形の部分バージョンも候補に入れる
static SPACE_AFTER_OPERATOR: LazyLock<Regex> = LazyLock::new(|| {
    let ws = whitespace_class();
    regex(&format!(
        "({ws}?){OPERATOR}{ws}?({}|{})",
        version_pattern(),
        strict_partial_version_pattern()
    ))
});

static SPACE_AFTER_TILDE: LazyLock<Regex> =
    LazyLock::new(|| regex(&format!("({ws}?)(?:~>?){ws}", ws = whitespace_class())));

static SPACE_AFTER_CARET: LazyLock<Regex> =
    LazyLock::new(|| regex(&format!(r"({ws}?)(?:\^){ws}", ws = whitespace_class())));

static TILDE: LazyLock<Regex> =
    LazyLock::new(|| regex(&format!("^(?:~>?){}$", partial_version_pattern())));

static CARET: LazyLock<Regex> =
    LazyLock::new(|| regex(&format!(r"^(?:\^){}$", partial_version_pattern())));

static PARTIAL: LazyLock<Regex> = LazyLock::new(|| {
    regex(&format!(
        "^{OPERATOR}{}?{}$",
        whitespace_class(),
        partial_version_pattern()
    ))
});

static STAR: LazyLock<Regex> =
    LazyLock::new(|| regex(&format!(r"(<|>)?=?{}?\*", whitespace_class())));

static ZERO_LOWER_BOUND: LazyLock<Regex> = LazyLock::new(|| {
    regex(&format!(
        r"^{ws}?>={ws}?0\.0\.0{ws}?$",
        ws = whitespace_class()
    ))
});

#[derive(Clone)]
pub struct VersionRange {
    alternatives: Vec<Vec<Comparator>>,
}

impl VersionRange {
    pub(crate) fn parse(input: &str) -> Result<Self, VersionRangeParseError> {
        // npm は、範囲の一部に読めない書き方があっても、全体が読めない場合以外は受け付ける
        let mut alternatives = Vec::new();
        for alternative in collapse_whitespace(input).split("||") {
            let comparators = parse_alternative(trim_whitespace(alternative)).ok_or_else(|| {
                VersionRangeParseError::new(input, "contains a version that cannot be parsed")
            })?;
            if !comparators.is_empty() {
                alternatives.push(comparators);
            }
        }
        if alternatives.is_empty() {
            return Err(VersionRangeParseError::new(input, "no valid range"));
        }

        if alternatives.len() > 1 {
            let first = alternatives[0].clone();
            alternatives.retain(|alternative| !alternative[0].is_null_set());
            if alternatives.is_empty() {
                alternatives = vec![first];
            } else if alternatives.len() > 1
                && let Some(any) = alternatives
                    .iter()
                    .find(|alternative| alternative.len() == 1 && alternative[0].is_any())
            {
                alternatives = vec![any.clone()];
            }
        }

        Ok(Self { alternatives })
    }

    pub(crate) fn satisfies(&self, version: &Version) -> bool {
        self.alternatives
            .iter()
            .any(|alternative| alternative_satisfies(alternative, version))
    }
}

impl fmt::Display for VersionRange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let formatted = self
            .alternatives
            .iter()
            .map(|alternative| {
                alternative
                    .iter()
                    .map(|comparator| comparator.to_string())
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .collect::<Vec<_>>()
            .join("||");
        if formatted.is_empty() {
            f.write_str("*")
        } else {
            f.write_str(&formatted)
        }
    }
}

fn parse_alternative(alternative: &str) -> Option<Vec<Comparator>> {
    let alternative = BUILD_METADATA.replace_all(alternative, "");
    let alternative = HYPHEN_RANGE.replace(&alternative, expand_hyphen);
    let alternative = SPACE_AFTER_OPERATOR.replace_all(&alternative, "${1}${2}${3}");
    let alternative = SPACE_AFTER_TILDE.replace_all(&alternative, "${1}~");
    let alternative = SPACE_AFTER_CARET.replace_all(&alternative, "${1}^");

    let expanded = alternative
        .split(' ')
        .map(expand)
        .collect::<Vec<_>>()
        .join(" ");
    let tokens: Vec<String> = split_on_whitespace(&expanded)
        .into_iter()
        .map(remove_zero_lower_bound)
        .filter(|token| has_comparator_form(token))
        .collect();

    // 条件の形をしているのにバージョンとして読めないものは、捨てずに範囲全体を読めないものにする（npm と同じ）
    let comparators = tokens
        .iter()
        .map(|token| Comparator::parse(token))
        .collect::<Option<Vec<_>>>()?;

    if let Some(null_set) = comparators
        .iter()
        .find(|comparator| comparator.is_null_set())
    {
        return Some(vec![null_set.clone()]);
    }

    let mut unique: Vec<Comparator> = Vec::new();
    for comparator in comparators {
        let value = comparator.to_string();
        if !unique.iter().any(|kept| kept.to_string() == value) {
            unique.push(comparator);
        }
    }
    if unique.len() > 1 {
        unique.retain(|comparator| !comparator.is_any());
    }
    Some(unique)
}

fn split_on_whitespace(text: &str) -> Vec<&str> {
    WHITESPACE_RUN.split(text).collect()
}

fn expand(token: &str) -> String {
    let token = map_words(trim_whitespace(token), expand_caret);
    let token = map_words(trim_whitespace(&token), expand_tilde);
    let token = map_words(&token, expand_partial);
    remove_star(&token)
}

fn map_words(text: &str, expand: fn(&str) -> String) -> String {
    split_on_whitespace(text)
        .into_iter()
        .map(expand)
        .collect::<Vec<_>>()
        .join(" ")
}

fn group<'a>(captures: &Captures<'a>, index: usize) -> &'a str {
    captures.get(index).map_or("", |matched| matched.as_str())
}

fn is_wildcard(part: &str) -> bool {
    part.is_empty() || part.eq_ignore_ascii_case("x") || part == "*"
}

// npm は桁を倍精度浮動小数点数として1を足すので、大きな数では丸めが起き、
// 1e21 以上は指数表記（`1e+21`）になる（その結果、条件として読めずに捨てられる）
fn increment(digits: &str) -> String {
    let number: f64 = digits.parse().expect("桁は数字だけ");
    let incremented = number + 1.0;
    if incremented >= 1e21 {
        format!("{incremented:e}").replacen('e', "e+", 1)
    } else {
        format!("{incremented}")
    }
}

fn expand_caret(word: &str) -> String {
    CARET
        .replace(word, |captures: &Captures| {
            let major = group(captures, 1);
            let minor = group(captures, 2);
            let patch = group(captures, 3);
            let prerelease = group(captures, 4);
            if is_wildcard(major) {
                String::new()
            } else if is_wildcard(minor) {
                format!(">={major}.0.0 <{}.0.0-0", increment(major))
            } else if is_wildcard(patch) {
                if major == "0" {
                    format!(">={major}.{minor}.0 <{major}.{}.0-0", increment(minor))
                } else {
                    format!(">={major}.{minor}.0 <{}.0.0-0", increment(major))
                }
            } else {
                let lower = if prerelease.is_empty() {
                    format!(">={major}.{minor}.{patch}")
                } else {
                    format!(">={major}.{minor}.{patch}-{prerelease}")
                };
                let upper = if major == "0" && minor == "0" {
                    format!("<{major}.{minor}.{}-0", increment(patch))
                } else if major == "0" {
                    format!("<{major}.{}.0-0", increment(minor))
                } else {
                    format!("<{}.0.0-0", increment(major))
                };
                format!("{lower} {upper}")
            }
        })
        .into_owned()
}

fn expand_tilde(word: &str) -> String {
    TILDE
        .replace(word, |captures: &Captures| {
            let major = group(captures, 1);
            let minor = group(captures, 2);
            let patch = group(captures, 3);
            let prerelease = group(captures, 4);
            if is_wildcard(major) {
                String::new()
            } else if is_wildcard(minor) {
                format!(">={major}.0.0 <{}.0.0-0", increment(major))
            } else if is_wildcard(patch) {
                format!(">={major}.{minor}.0 <{major}.{}.0-0", increment(minor))
            } else if prerelease.is_empty() {
                format!(
                    ">={major}.{minor}.{patch} <{major}.{}.0-0",
                    increment(minor)
                )
            } else {
                format!(
                    ">={major}.{minor}.{patch}-{prerelease} <{major}.{}.0-0",
                    increment(minor)
                )
            }
        })
        .into_owned()
}

fn expand_partial(word: &str) -> String {
    PARTIAL
        .replace(trim_whitespace(word), |captures: &Captures| {
            let major = group(captures, 2);
            let minor = group(captures, 3);
            let patch = group(captures, 4);
            let any_major = is_wildcard(major);
            let any_minor = any_major || is_wildcard(minor);
            let any_patch = any_minor || is_wildcard(patch);
            let operator = match group(captures, 1) {
                "=" if any_patch => "",
                operator => operator,
            };

            if any_major {
                if operator == ">" || operator == "<" {
                    "<0.0.0-0".to_string()
                } else {
                    "*".to_string()
                }
            } else if !operator.is_empty() && any_patch {
                let mut major = major.to_string();
                let mut minor = if any_minor { "0" } else { minor }.to_string();
                let operator = match operator {
                    ">" => {
                        if any_minor {
                            major = increment(&major);
                        } else {
                            minor = increment(&minor);
                        }
                        ">="
                    }
                    "<=" => {
                        if any_minor {
                            major = increment(&major);
                        } else {
                            minor = increment(&minor);
                        }
                        "<"
                    }
                    operator => operator,
                };
                let prerelease = if operator == "<" { "-0" } else { "" };
                format!("{operator}{major}.{minor}.0{prerelease}")
            } else if any_minor {
                format!(">={major}.0.0 <{}.0.0-0", increment(major))
            } else if any_patch {
                format!(">={major}.{minor}.0 <{major}.{}.0-0", increment(minor))
            } else {
                group(captures, 0).to_string()
            }
        })
        .into_owned()
}

fn remove_star(text: &str) -> String {
    STAR.replace(trim_whitespace(text), "").into_owned()
}

fn remove_zero_lower_bound(token: &str) -> String {
    ZERO_LOWER_BOUND
        .replace(trim_whitespace(token), "")
        .into_owned()
}

fn expand_hyphen(captures: &Captures) -> String {
    let from = group(captures, 1);
    let from_major = group(captures, 2);
    let from_minor = group(captures, 3);
    let from_patch = group(captures, 4);
    let to = group(captures, 7);
    let to_major = group(captures, 8);
    let to_minor = group(captures, 9);
    let to_patch = group(captures, 10);
    let to_prerelease = group(captures, 11);

    let lower = if is_wildcard(from_major) {
        String::new()
    } else if is_wildcard(from_minor) {
        format!(">={from_major}.0.0")
    } else if is_wildcard(from_patch) {
        format!(">={from_major}.{from_minor}.0")
    } else {
        format!(">={from}")
    };
    let upper = if is_wildcard(to_major) {
        String::new()
    } else if is_wildcard(to_minor) {
        format!("<{}.0.0-0", increment(to_major))
    } else if is_wildcard(to_patch) {
        format!("<{to_major}.{}.0-0", increment(to_minor))
    } else if !to_prerelease.is_empty() {
        format!("<={to_major}.{to_minor}.{to_patch}-{to_prerelease}")
    } else {
        format!("<={to}")
    };
    trim_whitespace(&format!("{lower} {upper}")).to_string()
}

fn alternative_satisfies(alternative: &[Comparator], version: &Version) -> bool {
    if !alternative
        .iter()
        .all(|comparator| comparator.matches(version))
    {
        return false;
    }
    if !version.is_prerelease() {
        return true;
    }

    // prerelease のバージョンは、同じ major.minor.patch の prerelease を条件側が明示しているときだけ満たす。
    // ^1.2.3-pr.1 は 1.2.3-pr.2 を満たすが、範囲の内側にある 1.2.4-alpha は満たさない
    alternative.iter().any(|comparator| match comparator {
        Comparator::Constraint {
            version: allowed, ..
        } => allowed.is_prerelease() && allowed.has_same_core(version),
        Comparator::Any => false,
    })
}

#[derive(Debug, thiserror::Error)]
#[error("invalid version range {input:?}: {reason}")]
pub struct VersionRangeParseError {
    input: String,
    reason: String,
}

impl VersionRangeParseError {
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

    const RANGES: &str = include_str!("../../tests/fixtures/semver/ranges.json");
    const SATISFIES: &str = include_str!("../../tests/fixtures/semver/satisfies.json");

    #[test]
    fn range_parse_matches_npm() {
        let entries = load_fixture(RANGES);
        let mut failures = Vec::new();

        for entry in &entries {
            let input = entry["input"].as_str().expect("input は文字列");
            let expected = entry["normalized"].as_str();
            let actual = VersionRange::parse(input)
                .ok()
                .map(|range| range.to_string());
            if actual.as_deref() != expected {
                failures.push(format!(
                    "{input:?}: npm = {expected:?}, peerdoctor = {actual:?}"
                ));
            }
        }

        assert_all_match(&failures, entries.len());
    }

    #[test]
    fn range_satisfies_matches_npm() {
        let entries = load_fixture(SATISFIES);
        let mut failures = Vec::new();

        for entry in &entries {
            let range = entry["range"].as_str().expect("range は文字列");
            let version = entry["version"].as_str().expect("version は文字列");
            let expected = entry["expected"].as_bool().expect("expected は真偽値");
            let actual = VersionRange::parse(range)
                .expect("テストデータの範囲は読める")
                .satisfies(&Version::parse(version).expect("テストデータのバージョンは読める"));
            if actual != expected {
                failures.push(format!(
                    "{range:?} を {version:?} が満たすか: npm = {expected}, peerdoctor = {actual}"
                ));
            }
        }

        assert_all_match(&failures, entries.len());
    }
}
