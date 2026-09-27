# semver のテストデータ

peerdoctor のバージョン・範囲の判定（`src/version.rs`）が、npm と同じ答えを返すかを確かめるためのデータ。

## 期待値の出どころ

npm 11 が同梱している semver 7.8.1 を、**loose モード（`includePrerelease` なし）**で呼んで求めた。
npm が peer を含む依存の判定で semver を呼ぶときと同じ条件
（`@npmcli/arborist` の `dep-valid.js`、`npm-package-arg`）。

## 入力の出どころ

- node-semver 7.8.1 の公式テストデータ（`test/fixtures/` の `range-include`、`range-exclude`、
  `range-parse`、`valid-versions`、`invalid-versions`、`comparisons`、`equality`）。
  各データに付いているオプションは使わず、範囲とバージョンの文字列だけを入力として使う。
  文字列以外の入力（正規表現など）は対象外
- peerdoctor の調査で見つけた境界のケース（`scripts/semver-fixtures/generate.js` の
  `EDGE_RANGES` と `EDGE_VERSIONS`）。範囲 × バージョンの総当たり

## ファイル

| ファイル | 1件の形 | 期待値の求め方 |
|---|---|---|
| `versions.json` | `{ input, normalized }` | `semver.valid(input, { loose: true })`。バージョンとして読めなければ `null` |
| `ranges.json` | `{ input, normalized }` | `semver.validRange(input, { loose: true })`。範囲として読めなければ `null` |
| `compare.json` | `{ a, b, expected }` | `semver.compare(a, b, { loose: true })`。`a < b` なら -1、同じなら 0、`a > b` なら 1 |
| `satisfies.json` | `{ range, version, expected }` | `semver.satisfies(version, range, { loose: true })`。範囲・バージョンとも読めるものだけ |

## 作り直し方

```bash
cd scripts/semver-fixtures
npm ci
npm run generate
```

Node.js 18 以上が必要（`fetch` を使う）。公式テストデータを GitHub から取得するので、ネットワークも必要。
生成結果は決定的で、同じ semver の版なら何度作り直しても同じ内容になる。

## ライセンス

入力の一部は node-semver のテストデータに由来する。node-semver は ISC ライセンス
（`LICENSE-node-semver`）。
