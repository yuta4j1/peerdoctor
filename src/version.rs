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

use regex::{Captures, Regex};

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

// JS の `trim()`
fn js_trim(text: &str) -> &str {
    text.trim_matches(is_js_whitespace)
}

// JS の `split(/\s+/)`。連続する空白を1つの区切りとして扱い、先頭や末尾に空白があれば空文字が入る
fn js_split_whitespace(text: &str) -> Vec<&str> {
    JS_WHITESPACE_RUN.split(text).collect()
}

// JS の `trim().replace(/\s+/g, ' ')`（前後の空白を取り、連続する空白を1つの半角スペースにする）
fn collapse_js_whitespace(text: &str) -> String {
    text.split(is_js_whitespace)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

// node-semver の internal/re.js の部品。名前は re.js に揃えている。
// JS の \d は 0-9 だけを指すので [0-9] と書く（Rust の \d は他の文字種の数字も含む）。
// JS 版は、空白をまとめた入力に対しては `\s*` を `\s{0,1}`、`\s+` を `\s{1,1}` に置き換えた
// 正規表現（safeRe）を使っているので、ここでもそれに合わせる。
const NUMERIC_IDENTIFIER: &str = "0|[1-9][0-9]*";
const NUMERIC_IDENTIFIER_LOOSE: &str = "[0-9]+";
const NON_NUMERIC_IDENTIFIER: &str = "[0-9]*[a-zA-Z-][a-zA-Z0-9-]*";
const BUILD_IDENTIFIER: &str = "[a-zA-Z0-9-]+";
const GTLT: &str = "((?:<|>)?=?)";

fn ws() -> String {
    format!("[{JS_WHITESPACE}]")
}

// re.js の BUILD。キャプチャは 1 つ（ビルドメタデータ）
fn build() -> String {
    format!(r"(?:\+({BUILD_IDENTIFIER}(?:\.{BUILD_IDENTIFIER})*))")
}

// re.js の PRERELEASE（strict）と PRERELEASELOOSE。キャプチャは 1 つ（prerelease）
fn prerelease() -> String {
    let identifier = format!("(?:{NON_NUMERIC_IDENTIFIER}|{NUMERIC_IDENTIFIER})");
    format!(r"(?:-({identifier}(?:\.{identifier})*))")
}

fn prerelease_loose() -> String {
    let identifier = format!("(?:{NON_NUMERIC_IDENTIFIER}|{NUMERIC_IDENTIFIER_LOOSE})");
    format!(r"(?:-?({identifier}(?:\.{identifier})*))")
}

// re.js の LOOSEPLAIN（前後の ^ と $ が無い、バージョン部分だけの形）。
// キャプチャは 1〜3 が major / minor / patch、4 が prerelease、5 がビルドメタデータ
fn loose_plain() -> String {
    let n = NUMERIC_IDENTIFIER_LOOSE;
    format!(
        r"[v={JS_WHITESPACE}]*({n})\.({n})\.({n}){}?{}?",
        prerelease_loose(),
        build()
    )
}

// re.js の XRANGEPLAIN（strict）と XRANGEPLAINLOOSE。`1.x` や `1.2` のような部分バージョンも含む。
// キャプチャは 1〜3 が major / minor / patch（無ければ不参加）、4 が prerelease、5 がビルドメタデータ
fn xrange_plain() -> String {
    let x = format!(r"{NUMERIC_IDENTIFIER}|x|X|\*");
    format!(
        r"[v={JS_WHITESPACE}]*({x})(?:\.({x})(?:\.({x})(?:{})?{}?)?)?",
        prerelease(),
        build()
    )
}

fn xrange_plain_loose() -> String {
    let x = format!(r"{NUMERIC_IDENTIFIER_LOOSE}|x|X|\*");
    format!(
        r"[v={JS_WHITESPACE}]*({x})(?:\.({x})(?:\.({x})(?:{})?{}?)?)?",
        prerelease_loose(),
        build()
    )
}

fn regex(pattern: &str) -> Regex {
    Regex::new(pattern).expect("node-semver から移植した正しい正規表現")
}

static JS_WHITESPACE_RUN: LazyLock<Regex> = LazyLock::new(|| regex(&format!("{}+", ws())));

// re.js の LOOSE
static LOOSE_VERSION: LazyLock<Regex> = LazyLock::new(|| regex(&format!("^{}$", loose_plain())));

// re.js の COMPARATORLOOSE。キャプチャは 1 が演算子、2 がバージョン部分。空文字にも一致する（`|^$`）
static COMPARATOR_LOOSE: LazyLock<Regex> =
    LazyLock::new(|| regex(&format!("^{GTLT}{}{{0,1}}({})$|^$", ws(), loose_plain())));

// range.js の BUILDSTRIPRE（範囲の中のビルドメタデータをすべて取り除く）
static BUILD_STRIP: LazyLock<Regex> = LazyLock::new(|| regex(&build()));

// re.js の HYPHENRANGELOOSE（`1.2.3 - 2.0.0`）。キャプチャは 1 が左側全体、2〜6 がその部品、
// 7 が右側全体、8〜12 がその部品
static HYPHEN_RANGE_LOOSE: LazyLock<Regex> = LazyLock::new(|| {
    let (ws, plain) = (ws(), xrange_plain_loose());
    regex(&format!(
        "^{ws}{{0,1}}({plain}){ws}{{1,1}}-{ws}{{1,1}}({plain}){ws}{{0,1}}$"
    ))
});

// re.js の COMPARATORTRIM（`> 1.2.3` → `>1.2.3`）。キャプチャ 1〜3 をつなげて置き換える
static COMPARATOR_TRIM: LazyLock<Regex> = LazyLock::new(|| {
    let ws = ws();
    regex(&format!(
        "({ws}{{0,1}}){GTLT}{ws}{{0,1}}({}|{})",
        loose_plain(),
        xrange_plain()
    ))
});

// re.js の TILDETRIM（`~ 1.2.3` → `~1.2.3`）と CARETTRIM（`^ 1.2.3` → `^1.2.3`）
static TILDE_TRIM: LazyLock<Regex> =
    LazyLock::new(|| regex(&format!("({ws}{{0,1}})(?:~>?){ws}{{1,1}}", ws = ws())));
static CARET_TRIM: LazyLock<Regex> =
    LazyLock::new(|| regex(&format!(r"({ws}{{0,1}})(?:\^){ws}{{1,1}}", ws = ws())));

// re.js の TILDELOOSE、CARETLOOSE、XRANGELOOSE。
// キャプチャは XRANGEPLAINLOOSE と同じ並び（XRANGELOOSE だけ、先頭に演算子のキャプチャが1つ増える）
static TILDE_LOOSE: LazyLock<Regex> =
    LazyLock::new(|| regex(&format!("^(?:~>?){}$", xrange_plain_loose())));
static CARET_LOOSE: LazyLock<Regex> =
    LazyLock::new(|| regex(&format!(r"^(?:\^){}$", xrange_plain_loose())));
static XRANGE_LOOSE: LazyLock<Regex> =
    LazyLock::new(|| regex(&format!("^{GTLT}{}{{0,1}}{}$", ws(), xrange_plain_loose())));

// re.js の STAR（`*`、`>=*` など）と GTE0（`>=0.0.0`）。どちらも「何でもよい」に置き換える
static STAR: LazyLock<Regex> = LazyLock::new(|| regex(&format!(r"(<|>)?=?{}{{0,1}}\*", ws())));
static GTE0: LazyLock<Regex> = LazyLock::new(|| {
    regex(&format!(
        r"^{ws}{{0,1}}>={ws}{{0,1}}0\.0\.0{ws}{{0,1}}$",
        ws = ws()
    ))
});

// ビルドメタデータ（`+build.5`）は読み取るが、比較にも表示にも使わないので持たない（JS 版と同じ扱い）
#[derive(Clone)]
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
#[derive(Clone, PartialEq)]
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
#[derive(Clone)]
enum Comparator {
    // 空の条件。どのバージョンでも満たす（JS 版の ANY）
    Any,
    Constraint {
        operator: Operator,
        version: Version,
    },
}

#[derive(Clone)]
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
        let normalized = collapse_js_whitespace(input);
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

    // range.js の isAny（空の条件。どのバージョンでも満たす）
    fn is_any(&self) -> bool {
        matches!(self, Self::Any)
    }

    // range.js の isNullSet（`<0.0.0-0`。どのバージョンも満たさない条件）。
    // JS 版と同じく、正規化した形で見分ける
    fn is_null_set(&self) -> bool {
        self.to_string() == "<0.0.0-0"
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

// node-semver の Range（loose モード）。`||` で区切られた選択肢のどれかを満たせばよい。
// 各選択肢は条件（Comparator）の並びで、そのすべてを満たす必要がある
#[derive(Clone)]
pub struct VersionRange {
    sets: Vec<Vec<Comparator>>,
}

impl VersionRange {
    // node-semver の Range のコンストラクタ
    pub(crate) fn parse(input: &str) -> Result<Self, VersionRangeParseError> {
        let raw = collapse_js_whitespace(input);

        // `||` で区切り、各部分を条件の並びにする。条件が1つも残らなかった部分は捨てる
        // （loose モードでは、範囲の一部が不正でも、全体が不正でなければ許される）
        let mut sets = Vec::new();
        for part in raw.split("||") {
            let comparators = parse_part(js_trim(part)).ok_or_else(|| {
                VersionRangeParseError::new(input, "contains a version that cannot be parsed")
            })?;
            if !comparators.is_empty() {
                sets.push(comparators);
            }
        }
        if sets.is_empty() {
            return Err(VersionRangeParseError::new(input, "no valid range"));
        }

        // 選択肢が複数あるとき、「どれも満たさない」選択肢は捨てる（全部そうなら最初の1つを残す）。
        // 残りに「何でもよい」選択肢があれば、範囲全体を「何でもよい」にする
        if sets.len() > 1 {
            let first = sets[0].clone();
            sets.retain(|set| !set[0].is_null_set());
            if sets.is_empty() {
                sets = vec![first];
            } else if sets.len() > 1
                && let Some(any) = sets.iter().find(|set| set.len() == 1 && set[0].is_any())
            {
                sets = vec![any.clone()];
            }
        }

        Ok(Self { sets })
    }

    // node-semver の Range#test
    pub(crate) fn satisfies(&self, version: &Version) -> bool {
        self.sets.iter().any(|set| set_satisfies(set, version))
    }
}

// node-semver の validRange が返す、正規化した形（`>=1.2.0 <2.0.0-0||>=3.0.0`）。
// 何でもよい範囲は、JS 版の validRange と同じく `*` にする
impl fmt::Display for VersionRange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let formatted = self
            .sets
            .iter()
            .map(|set| {
                set.iter()
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

// range.js の parseRange。`||` で区切った1つの部分を、条件の並びにする。
// 条件の形はしているのにバージョンとして読めない（数が大きすぎるなど）ものがあれば None
// （JS 版は例外を投げ、範囲全体が不正になる）
fn parse_part(part: &str) -> Option<Vec<Comparator>> {
    // ビルドメタデータは範囲の意味に関係しないので、最初にすべて取り除く
    let part = BUILD_STRIP.replace_all(part, "");
    // `1.2.3 - 2.0.0` → `>=1.2.3 <=2.0.0`
    let part = HYPHEN_RANGE_LOOSE.replace(&part, replace_hyphen);
    // `> 1.2.3` → `>1.2.3`、`~ 1.2.3` → `~1.2.3`、`^ 1.2.3` → `^1.2.3`
    let part = COMPARATOR_TRIM.replace_all(&part, "${1}${2}${3}");
    let part = TILDE_TRIM.replace_all(&part, "${1}~");
    let part = CARET_TRIM.replace_all(&part, "${1}^");

    // 1つずつ `^` `~` `x` `*` を展開してから、空白で区切って条件の文字列の並びにする
    let expanded = part
        .split(' ')
        .map(parse_comparator)
        .collect::<Vec<_>>()
        .join(" ");
    let tokens: Vec<String> = js_split_whitespace(&expanded)
        .into_iter()
        .map(replace_gte0)
        // loose モードでは、条件の形をしていないものは捨てる
        .filter(|token| COMPARATOR_LOOSE.is_match(token))
        .collect();

    let comparators = tokens
        .iter()
        .map(|token| Comparator::parse(token))
        .collect::<Option<Vec<_>>>()?;

    // 「どれも満たさない」条件があれば、その部分はそれだけにする
    if let Some(null_set) = comparators
        .iter()
        .find(|comparator| comparator.is_null_set())
    {
        return Some(vec![null_set.clone()]);
    }

    // 同じ条件は1つにまとめる。条件が複数あるなら「何でもよい」は取り除く
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

// range.js の parseComparator（`^` `~` `x` `*` を、演算子つきの条件に展開する）。
// JS 版はここでもビルドメタデータを取り除くが、parse_part で取り除き済みなので省く
fn parse_comparator(comp: &str) -> String {
    let comp = map_tokens(js_trim(comp), replace_caret);
    let comp = map_tokens(js_trim(&comp), replace_tilde);
    let comp = map_tokens(&comp, replace_x_range);
    replace_stars(&comp)
}

// 空白で区切った1つずつに replace をかけ、半角スペースでつなぎ直す
fn map_tokens(comp: &str, replace: fn(&str) -> String) -> String {
    js_split_whitespace(comp)
        .into_iter()
        .map(replace)
        .collect::<Vec<_>>()
        .join(" ")
}

// キャプチャの値。参加しなかったキャプチャは空文字にする（JS 版の undefined と同じ扱いになる）
fn group<'a>(captures: &Captures<'a>, index: usize) -> &'a str {
    captures.get(index).map_or("", |matched| matched.as_str())
}

// range.js の isX（部分バージョンの省略された桁や、x / X / *）
fn is_x(identifier: &str) -> bool {
    identifier.is_empty() || identifier.eq_ignore_ascii_case("x") || identifier == "*"
}

// JS の `+digits + 1` を文字列にしたもの。JS 版は桁を数値（浮動小数点数）に変換して1を足し、
// 文字列に戻すので、大きな数では丸めや指数表記（`1e+21`）も JS と同じになるようにする
fn js_increment(digits: &str) -> String {
    let number: f64 = digits.parse().expect("桁は数字だけ");
    let incremented = number + 1.0;
    if incremented >= 1e21 {
        format!("{incremented:e}").replacen('e', "e+", 1)
    } else {
        format!("{incremented}")
    }
}

// range.js の replaceCaret。`^1.2.3` → `>=1.2.3 <2.0.0-0`、`^0.1.2` → `>=0.1.2 <0.2.0-0`、
// `^0.0.1` → `>=0.0.1 <0.0.2-0` のように、左端の 0 でない桁が変わる手前までを許す
fn replace_caret(comp: &str) -> String {
    CARET_LOOSE
        .replace(comp, |captures: &Captures| {
            let major = group(captures, 1);
            let minor = group(captures, 2);
            let patch = group(captures, 3);
            let prerelease = group(captures, 4);
            if is_x(major) {
                String::new()
            } else if is_x(minor) {
                format!(">={major}.0.0 <{}.0.0-0", js_increment(major))
            } else if is_x(patch) {
                if major == "0" {
                    format!(">={major}.{minor}.0 <{major}.{}.0-0", js_increment(minor))
                } else {
                    format!(">={major}.{minor}.0 <{}.0.0-0", js_increment(major))
                }
            } else {
                let lower = if prerelease.is_empty() {
                    format!(">={major}.{minor}.{patch}")
                } else {
                    format!(">={major}.{minor}.{patch}-{prerelease}")
                };
                let upper = if major == "0" && minor == "0" {
                    format!("<{major}.{minor}.{}-0", js_increment(patch))
                } else if major == "0" {
                    format!("<{major}.{}.0-0", js_increment(minor))
                } else {
                    format!("<{}.0.0-0", js_increment(major))
                };
                format!("{lower} {upper}")
            }
        })
        .into_owned()
}

// range.js の replaceTilde。`~1.2.3` → `>=1.2.3 <1.3.0-0`、`~1` → `>=1.0.0 <2.0.0-0` のように、
// 指定された一番細かい桁の1つ上の桁が変わる手前までを許す
fn replace_tilde(comp: &str) -> String {
    TILDE_LOOSE
        .replace(comp, |captures: &Captures| {
            let major = group(captures, 1);
            let minor = group(captures, 2);
            let patch = group(captures, 3);
            let prerelease = group(captures, 4);
            if is_x(major) {
                String::new()
            } else if is_x(minor) {
                format!(">={major}.0.0 <{}.0.0-0", js_increment(major))
            } else if is_x(patch) {
                format!(">={major}.{minor}.0 <{major}.{}.0-0", js_increment(minor))
            } else if prerelease.is_empty() {
                format!(
                    ">={major}.{minor}.{patch} <{major}.{}.0-0",
                    js_increment(minor)
                )
            } else {
                format!(
                    ">={major}.{minor}.{patch}-{prerelease} <{major}.{}.0-0",
                    js_increment(minor)
                )
            }
        })
        .into_owned()
}

// range.js の replaceXRange。`1.x` → `>=1.0.0 <2.0.0-0`、`>1.2` → `>=1.3.0`、`<=1.x` → `<2.0.0-0`、
// `*` → 何でもよい、のように、省略された桁や x を含む条件を展開する
fn replace_x_range(comp: &str) -> String {
    XRANGE_LOOSE
        .replace(js_trim(comp), |captures: &Captures| {
            let major = group(captures, 2);
            let minor = group(captures, 3);
            let patch = group(captures, 4);
            let x_major = is_x(major);
            let x_minor = x_major || is_x(minor);
            let x_patch = x_minor || is_x(patch);
            let any_x = x_patch;
            let operator = match group(captures, 1) {
                "=" if any_x => "",
                operator => operator,
            };

            if x_major {
                // `>*` と `<*` は何も満たさない。それ以外（`*`、`>=*` など）は何でもよい
                if operator == ">" || operator == "<" {
                    "<0.0.0-0".to_string()
                } else {
                    "*".to_string()
                }
            } else if !operator.is_empty() && any_x {
                // 演算子つきで x を含むもの。x の桁を 0 にし、意味が変わらないよう演算子を調整する
                let mut major = major.to_string();
                let mut minor = if x_minor { "0" } else { minor }.to_string();
                let operator = match operator {
                    // `>1` → `>=2.0.0`、`>1.2` → `>=1.3.0`
                    ">" => {
                        if x_minor {
                            major = js_increment(&major);
                        } else {
                            minor = js_increment(&minor);
                        }
                        ">="
                    }
                    // `<=1` → `<2.0.0-0`、`<=1.2` → `<1.3.0-0`
                    "<=" => {
                        if x_minor {
                            major = js_increment(&major);
                        } else {
                            minor = js_increment(&minor);
                        }
                        "<"
                    }
                    operator => operator,
                };
                let prerelease = if operator == "<" { "-0" } else { "" };
                format!("{operator}{major}.{minor}.0{prerelease}")
            } else if x_minor {
                format!(">={major}.0.0 <{}.0.0-0", js_increment(major))
            } else if x_patch {
                format!(">={major}.{minor}.0 <{major}.{}.0-0", js_increment(minor))
            } else {
                group(captures, 0).to_string()
            }
        })
        .into_owned()
}

// range.js の replaceStars。`*` は他の条件と AND されるだけで意味が無いので取り除く
fn replace_stars(comp: &str) -> String {
    STAR.replace(js_trim(comp), "").into_owned()
}

// range.js の replaceGTE0。`>=0.0.0` は何でもよいのと同じなので取り除く
fn replace_gte0(comp: &str) -> String {
    GTE0.replace(js_trim(comp), "").into_owned()
}

// range.js の hyphenReplace。`1.2.3 - 2.3.4` → `>=1.2.3 <=2.3.4`、`1.2 - 2.3` → `>=1.2.0 <2.4.0-0`
fn replace_hyphen(captures: &Captures) -> String {
    let from = group(captures, 1);
    let from_major = group(captures, 2);
    let from_minor = group(captures, 3);
    let from_patch = group(captures, 4);
    let to = group(captures, 7);
    let to_major = group(captures, 8);
    let to_minor = group(captures, 9);
    let to_patch = group(captures, 10);
    let to_prerelease = group(captures, 11);

    // JS 版は左側の prerelease の有無でも分岐するが、includePrerelease なしではどちらも同じ結果になる
    let lower = if is_x(from_major) {
        String::new()
    } else if is_x(from_minor) {
        format!(">={from_major}.0.0")
    } else if is_x(from_patch) {
        format!(">={from_major}.{from_minor}.0")
    } else {
        format!(">={from}")
    };
    let upper = if is_x(to_major) {
        String::new()
    } else if is_x(to_minor) {
        format!("<{}.0.0-0", js_increment(to_major))
    } else if is_x(to_patch) {
        format!("<{to_major}.{}.0-0", js_increment(to_minor))
    } else if !to_prerelease.is_empty() {
        format!("<={to_major}.{to_minor}.{to_patch}-{to_prerelease}")
    } else {
        format!("<={to}")
    };
    js_trim(&format!("{lower} {upper}")).to_string()
}

// range.js の testSet
fn set_satisfies(set: &[Comparator], version: &Version) -> bool {
    if !set.iter().all(|comparator| comparator.matches(version)) {
        return false;
    }
    if version.prerelease.is_empty() {
        return true;
    }

    // prerelease のバージョンは、同じ major.minor.patch の prerelease を条件側が明示しているときだけ
    // 満たすとする。たとえば ^1.2.3-pr.1（>=1.2.3-pr.1 <2.0.0-0）は 1.2.3-pr.2 を満たすが、
    // 範囲の内側にある 1.2.4-alpha は満たさない
    set.iter().any(|comparator| match comparator {
        Comparator::Constraint {
            version: allowed, ..
        } => {
            !allowed.prerelease.is_empty()
                && (allowed.major, allowed.minor, allowed.patch)
                    == (version.major, version.minor, version.patch)
        }
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

    // 期待値は npm の semver 7.8.1（loose モード）で生成したもの。
    // 作り方は tests/fixtures/semver/README.md
    const VERSIONS: &str = include_str!("../tests/fixtures/semver/versions.json");
    const COMPARE: &str = include_str!("../tests/fixtures/semver/compare.json");
    const COMPARATORS: &str = include_str!("../tests/fixtures/semver/comparators.json");
    const RANGES: &str = include_str!("../tests/fixtures/semver/ranges.json");
    const SATISFIES: &str = include_str!("../tests/fixtures/semver/satisfies.json");

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

    #[test]
    fn range_parse_matches_npm() {
        let entries = load(RANGES);
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
        let entries = load(SATISFIES);
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
