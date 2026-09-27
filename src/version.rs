// バージョンと範囲の読み取り・比較・判定は、npm が使っている node-semver
// （https://github.com/npm/node-semver）の移植を含む。
// node-semver: ISC License, Copyright (c) Isaac Z. Schlueter and Contributors
// ライセンスの全文は tests/fixtures/semver/LICENSE-node-semver にある。
//
// 移植しているのは、npm が peer を含む依存の判定で semver を呼ぶときの条件
// （loose モード、includePrerelease なし）の挙動だけ。

use std::cmp::Ordering;
use std::fmt;
use std::sync::LazyLock;

use regex::Regex;

// node-semver の internal/constants.js
const MAX_LENGTH: usize = 256;
const MAX_SAFE_INTEGER: u64 = 9_007_199_254_740_991;

// JavaScript の空白（正規表現の \s と String.prototype.trim が対象にする文字）。
// Rust の \s や char::is_whitespace とは、U+0085 と U+FEFF の扱いが違う。
// 正規表現の文字クラス用の JS_WHITESPACE と is_js_whitespace は、同じ文字の集合を表す。
const JS_WHITESPACE: &str = r"\t\n\x0B\x0C\r \u{A0}\u{1680}\u{2000}-\u{200A}\u{2028}\u{2029}\u{202F}\u{205F}\u{3000}\u{FEFF}";

fn is_js_whitespace(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n' | '\u{0B}' | '\u{0C}' | '\r' | ' ' | '\u{A0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200A}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202F}'
                | '\u{205F}'
                | '\u{3000}'
                | '\u{FEFF}'
    )
}

// node-semver の internal/re.js のうち、loose モードでバージョンを読むのに使う部品。
// JS の \d は 0-9 だけを指すので [0-9] と書く（Rust の \d は他の文字種の数字も含む）。
const NUMERIC_IDENTIFIER_LOOSE: &str = "[0-9]+";
const NON_NUMERIC_IDENTIFIER: &str = "[0-9]*[a-zA-Z-][a-zA-Z0-9-]*";
const BUILD_IDENTIFIER: &str = "[a-zA-Z0-9-]+";

// re.js の LOOSEPLAIN（前後の ^ と $ が無い、バージョン部分だけの形）。
// キャプチャは 1〜3 が major / minor / patch、4 が prerelease、5 がビルドメタデータ
fn loose_plain() -> String {
    let main = format!(
        r"({NUMERIC_IDENTIFIER_LOOSE})\.({NUMERIC_IDENTIFIER_LOOSE})\.({NUMERIC_IDENTIFIER_LOOSE})"
    );
    let prerelease_identifier = format!("(?:{NON_NUMERIC_IDENTIFIER}|{NUMERIC_IDENTIFIER_LOOSE})");
    let prerelease = format!(r"(?:-?({prerelease_identifier}(?:\.{prerelease_identifier})*))");
    let build = format!(r"(?:\+({BUILD_IDENTIFIER}(?:\.{BUILD_IDENTIFIER})*))");
    format!("[v={JS_WHITESPACE}]*{main}{prerelease}?{build}?")
}

// re.js の LOOSE
static LOOSE_VERSION: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!("^{}$", loose_plain())).expect("LOOSE_VERSION は正しい正規表現")
});

// re.js の COMPARATORLOOSE。キャプチャは 1 が演算子、2 がバージョン部分。
// 空文字にも一致する（`|^$`）。JS 版は空白をまとめてから一致を見るので、
// 演算子の後ろの空白は多くても1つ（JS 版の safeRe も `\s*` を `\s{0,1}` に置き換えている）
static COMPARATOR_LOOSE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        "^((?:<|>)?=?)[{JS_WHITESPACE}]{{0,1}}({})$|^$",
        loose_plain()
    ))
    .expect("COMPARATOR_LOOSE は正しい正規表現")
});

// ビルドメタデータ（`+build.5`）は読み取るが、比較にも表示にも使わないので持たない（JS 版と同じ扱い）
pub struct Version {
    major: u64,
    minor: u64,
    patch: u64,
    prerelease: Vec<Identifier>,
}

impl Version {
    // node-semver の SemVer のコンストラクタ（loose モード）
    pub(crate) fn parse(input: &str) -> Result<Self, VersionParseError> {
        // JS の文字列の長さは UTF-16 の単位で数える
        if input.encode_utf16().count() > MAX_LENGTH {
            return Err(VersionParseError::new(
                input,
                &format!("longer than {MAX_LENGTH} characters"),
            ));
        }

        let captures = LOOSE_VERSION
            .captures(input.trim_matches(is_js_whitespace))
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
}

// JS 版は数値に変換してから、MAX_SAFE_INTEGER を超えていないかを確かめる
fn parse_number(input: &str, digits: &str, name: &str) -> Result<u64, VersionParseError> {
    digits
        .parse::<u64>()
        .ok()
        .filter(|number| *number <= MAX_SAFE_INTEGER)
        .ok_or_else(|| {
            VersionParseError::new(
                input,
                &format!("{name} version is larger than {MAX_SAFE_INTEGER}"),
            )
        })
}

// node-semver の SemVer#compare。ビルドメタデータは比べない
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

// 「等しい」は比較の結果が Equal であることとする（ビルドメタデータの違いは無視される）
impl PartialEq for Version {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for Version {}

// node-semver の SemVer#format。ビルドメタデータは含めない
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

// prerelease の識別子（`1.2.3-beta.2` の `beta` と `2`）。
// JS 版は、数字だけで MAX_SAFE_INTEGER 未満の識別子を数値に変換し、それ以外は文字列のまま持つ。
// その区別をそのまま型にしている。
#[derive(PartialEq)]
enum Identifier {
    Numeric(u64),
    Text(String),
}

impl Identifier {
    fn parse(identifier: &str) -> Self {
        if identifier.bytes().all(|byte| byte.is_ascii_digit())
            && let Ok(number) = identifier.parse::<u64>()
            && number < MAX_SAFE_INTEGER
        {
            return Self::Numeric(number);
        }
        Self::Text(identifier.to_string())
    }

    // JS の `+identifier` にあたる値。数字だけの識別子なら数値、そうでなければ None。
    // 大きすぎて Text のままの数字も、JS 版と同じく浮動小数点数として比べる
    fn as_js_number(&self) -> Option<f64> {
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

// node-semver の SemVer#comparePre
fn compare_prerelease(a: &[Identifier], b: &[Identifier]) -> Ordering {
    // prerelease が無い方が大きい（1.0.0-beta < 1.0.0）
    match (a.is_empty(), b.is_empty()) {
        (true, true) => return Ordering::Equal,
        (false, true) => return Ordering::Less,
        (true, false) => return Ordering::Greater,
        (false, false) => {}
    }

    // 先頭から比べ、最初に違う識別子で決める。JS 版と同じく、違う識別子を
    // compare_identifiers が「等しい」と判定した場合も、そこで比較を終える
    for (x, y) in a.iter().zip(b) {
        if x != y {
            return compare_identifiers(x, y);
        }
    }
    // 共通部分が同じなら、識別子が多い方が大きい（1.0.0-beta < 1.0.0-beta.1）
    a.len().cmp(&b.len())
}

// node-semver の internal/identifiers.js の compareIdentifiers。
// 数字だけの識別子は数値として比べ、英字を含む識別子より小さいとする
fn compare_identifiers(a: &Identifier, b: &Identifier) -> Ordering {
    if let (Identifier::Numeric(x), Identifier::Numeric(y)) = (a, b) {
        return x.cmp(y);
    }
    match (a.as_js_number(), b.as_js_number()) {
        (Some(x), Some(y)) => x.total_cmp(&y),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        // どちらも英字を含む識別子なので、文字列として比べる
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

// node-semver の Comparator（loose モード）。`>=1.2.3` のような、演算子とバージョンの組を1つ表す。
// 範囲（VersionRange）を組み立てる部品
enum Comparator {
    // 空の条件。どのバージョンでも満たす（JS 版の ANY）
    Any,
    Constraint {
        operator: Operator,
        version: Version,
    },
}

enum Operator {
    Less,
    LessOrEqual,
    Greater,
    GreaterOrEqual,
    // JS 版では `=` も演算子なしも、これになる
    Equal,
}

impl Comparator {
    // 読めなければ None（JS 版は例外を投げる）
    fn parse(input: &str) -> Option<Self> {
        // JS 版は、前後の空白を取り、連続する空白を1つにまとめてから読む
        let normalized = input
            .split(is_js_whitespace)
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        let captures = COMPARATOR_LOOSE.captures(&normalized)?;

        // 空文字に一致した（バージョン部分が無い）なら、どのバージョンでも満たす
        let Some(version) = captures.get(2) else {
            return Some(Self::Any);
        };
        let operator = match captures.get(1).map_or("", |matched| matched.as_str()) {
            "<" => Operator::Less,
            "<=" => Operator::LessOrEqual,
            ">" => Operator::Greater,
            ">=" => Operator::GreaterOrEqual,
            // 残りは "" と "="（正規表現の形から、これ以外は来ない）
            _ => Operator::Equal,
        };
        let version = Version::parse(version.as_str()).ok()?;
        Some(Self::Constraint { operator, version })
    }

    // node-semver の Comparator#test
    fn matches(&self, version: &Version) -> bool {
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
}

// node-semver の Comparator#value（`>=1.2.3` のような正規化した形。空の条件は空文字）
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

#[cfg(test)]
mod tests {
    use super::*;

    // 期待値は npm の semver 7.8.1（loose モード）で生成したもの。
    // 作り方は tests/fixtures/semver/README.md
    const VERSIONS: &str = include_str!("../tests/fixtures/semver/versions.json");
    const COMPARE: &str = include_str!("../tests/fixtures/semver/compare.json");
    const COMPARATORS: &str = include_str!("../tests/fixtures/semver/comparators.json");

    fn load(json: &str) -> Vec<serde_json::Value> {
        serde_json::from_str(json).expect("テストデータは正しい JSON")
    }

    // 最初の1件で止めず、npm と違ったものをすべて並べて失敗させる
    fn assert_all_match(failures: &[String], total: usize) {
        assert!(
            failures.is_empty(),
            "{} / {} 件が npm と違う:\n{}",
            failures.len(),
            total,
            failures.join("\n")
        );
    }

    #[test]
    fn parse_matches_npm() {
        let entries = load(VERSIONS);
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
        let entries = load(COMPARE);
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

    #[test]
    fn comparator_matches_npm() {
        let entries = load(COMPARATORS);
        let mut failures = Vec::new();

        for entry in &entries {
            let input = entry["input"].as_str().expect("input は文字列");

            // 読み取り: 読めるか、読めたら正規化した形が同じか
            let comparator = Comparator::parse(input);
            let expected_value = entry["value"].as_str();
            let actual_value = comparator.as_ref().map(|comparator| comparator.to_string());
            if actual_value.as_deref() != expected_value {
                failures.push(format!(
                    "{input:?} の読み取り: npm = {expected_value:?}, peerdoctor = {actual_value:?}"
                ));
                continue;
            }

            // 判定: npm が「満たす」「満たさない」としたバージョンで、同じ答えになるか
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
