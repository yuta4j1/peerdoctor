// node-semver（https://github.com/npm/node-semver）をもとにしている。
// ISC License, Copyright (c) Isaac Z. Schlueter and Contributors
// 全文は tests/fixtures/semver/LICENSE-node-semver

mod comparator;
mod range;
#[expect(
    clippy::module_inception,
    reason = "中のモジュールは非公開で、外には pub use した型だけを見せる"
)]
mod version;

pub use range::VersionRange;
pub use version::Version;

use regex::Regex;

// npm の空白は Rust の char::is_whitespace と違い、U+FEFF を含み、U+0085 を含まない
const WHITESPACE: &str = r"\t\n\x0B\x0C\r \u{A0}\u{1680}\u{2000}-\u{200A}\u{2028}\u{2029}\u{202F}\u{205F}\u{3000}\u{FEFF}";

fn is_whitespace(c: char) -> bool {
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

fn trim_whitespace(text: &str) -> &str {
    text.trim_matches(is_whitespace)
}

fn collapse_whitespace(text: &str) -> String {
    text.split(is_whitespace)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

// Rust の \d は他の文字種の数字も含むので、[0-9] と書く
const NUMBER: &str = "[0-9]+";
const WORD: &str = "[0-9]*[a-zA-Z-][a-zA-Z0-9-]*";
const BUILD_IDENTIFIER: &str = "[a-zA-Z0-9-]+";
const OPERATOR: &str = "((?:<|>)?=?)";

fn whitespace_class() -> String {
    format!("[{WHITESPACE}]")
}

fn build_pattern() -> String {
    format!(r"(?:\+({BUILD_IDENTIFIER}(?:\.{BUILD_IDENTIFIER})*))")
}

fn prerelease_pattern() -> String {
    let identifier = format!("(?:{WORD}|{NUMBER})");
    format!(r"(?:-?({identifier}(?:\.{identifier})*))")
}

fn version_pattern() -> String {
    format!(
        r"[v={WHITESPACE}]*({NUMBER})\.({NUMBER})\.({NUMBER}){}?{}?",
        prerelease_pattern(),
        build_pattern()
    )
}

fn regex(pattern: &str) -> Regex {
    Regex::new(pattern).expect("正しい正規表現")
}

#[cfg(test)]
fn load_fixture(json: &str) -> Vec<serde_json::Value> {
    serde_json::from_str(json).expect("テストデータは正しい JSON")
}

#[cfg(test)]
fn assert_all_match(failures: &[String], total: usize) {
    assert!(
        failures.is_empty(),
        "{} / {} 件が npm と違う:\n{}",
        failures.len(),
        total,
        failures.join("\n")
    );
}
