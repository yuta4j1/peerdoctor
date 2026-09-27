'use strict'

// tests/fixtures/semver/ の JSON を生成する。
//
// 期待値はすべて、npm 11 が同梱している semver 7.8.1 を loose モード
// （includePrerelease なし）で呼んで求める。npm が peer を含む依存の判定で
// semver を呼ぶときと同じ条件（@npmcli/arborist の dep-valid.js、npm-package-arg）。
//
// 入力の元は2つ:
// - node-semver 7.8.1 の公式テストデータ（GitHub のタグから取得）。
//   各データに付いているオプション（strict / includePrerelease など）は使わず、
//   範囲とバージョンの文字列だけを入力として使う
// - peerdoctor の調査で見つけた境界のケース（EDGE_RANGES / EDGE_VERSIONS）。
//   範囲 × バージョンの総当たりで期待値を作る

const fs = require('fs')
const path = require('path')
const semver = require('semver')
const Comparator = require('semver/classes/comparator')
const constants = require('semver/internal/constants')

const SEMVER_VERSION = require('semver/package.json').version
const OFFICIAL_FIXTURES_URL =
  `https://raw.githubusercontent.com/npm/node-semver/v${SEMVER_VERSION}/test/fixtures`
const OUT_DIR = path.join(__dirname, '..', '..', 'tests', 'fixtures', 'semver')
const LOOSE = { loose: true }

const EDGE_RANGES = [
  // よくある形
  '^14 || ^15', '^16.8.0 || ^17.0.0 || ^18.0.0', '^18.0.0 || ^19.0.0', '>=16 <=18',
  '^0.0.1', '^0.1.2', '~1.2', '~1.2.3', '1.x', '1.2.x', '*', 'x', '>=1.2.3 <2',
  '1.2.3 - 2.3', '1.2 - 2', '=1.2.3', '1.2.3', 'v1.2.3', '<2.0.0',
  '^1.0.0 || >=2.5.0 <3', '>= 1.2.3', '~> 1.2', '^1.2.3+build.5',
  // prerelease
  '>1.2.3-alpha.1', '^1.2.3-beta.2', '~1.2.3-beta.2', '1.2.3-beta.1 - 2',
  '>=1.2.3-beta.1 <1.2.3', '^1.2.3-beta.1 || ^2', '<=1.2.3-beta',
  // 部分バージョンとワイルドカード
  '<=11', '<11', '>11', '>=11', '<=1.2', '>1.2', '1.2.x - 2.x', '1.x - 2',
  '~1.2.x', '^1.x', '^0.x', '~0.0',
  // 空と || の部分
  '', ' ', '   ', '\t', '||', ' || ', '^1 ||', '|| ^1', '^1 || ', '^1 || || ^2',
  // 不正な部分（loose では捨てられる）
  'latest', 'next', 'abc ||', '|| abc', '1.2.3.4 ||', '^1 || abc', 'abc || def',
  // loose モード特有の書き方
  '1.2.3beta', '=v1.2.3', '~v1.2', '>= v1.2.3', '1.2.3alpha || 2', 'v 1.2.3',
  '>=1.2.3foo', '~1.2.3beta.1', '1.2.3 foo 2.0.0', '>=1.0.0 foo <2.0.0',
  // 空白区切りの AND（重なる / 重ならない）
  '^1.0.0 v1.2.3', '1.2.3 <2.0.0', '>=1.0.0 1.2.3', '^1.2 <1.5',
  '1.2.3 2.0.0', '^1 ^2', '>=2.0.0 <1.0.0', '<1.0.0 >=2.0.0', '1.x 2.x',
  '~1.2 ~1.3', '1 2', '=1.2.3 =2.0.0', '1.2.3 - 2.0.0 3.0.0', '^1.2 >=2.0.0',
]

const EDGE_VERSIONS = [
  '0.0.1', '0.0.2', '0.0.5', '0.1.5', '0.5.0', '1.2.3', '1.2.4', '1.3.0', '1.5.0',
  '1.2.3-alpha.2', '1.2.3-beta.2', '1.2.3-beta.3', '1.2.4-alpha',
  '2.0.0', '2.0.0-rc.1', '2.3.9', '2.6.0', '3.0.0',
  '11.0.0', '11.9.9', '12.0.0', '12.0.0-0',
  '14.2.0', '15.3.0', '16.0.0', '16.14.0', '17.0.2', '18.2.0', '19.0.0',
]

// バージョンの読み取りだけを確かめる入力（範囲との総当たりには使わない）
const EDGE_VERSION_INPUTS = [
  // 空白。U+FEFF は JS では空白だが Rust では空白ではない。U+0085 はその逆
  ' 1.2.3 ', '\t1.2.3\n', ' 1.2.3', '﻿1.2.3', '1.2.3﻿', '\u00851.2.3', '1.2.3\u0085',
  // loose モード特有の接頭辞
  'v 1.2.3', '=1.2.3', '== 1.2.3', 'v=1.2.3', '=v1.2.3', 'vv1.2.3',
  // 先頭のゼロ
  '01.02.03', '1.2.3-01', '1.2.3-0001.2', '1.2.3-0a',
  // 大きな数（JS の安全な整数 9007199254740991 の前後）
  '9007199254740991.0.0', '9007199254740992.0.0', '0.0.9007199254740991',
  '1.2.3-9007199254740990', '1.2.3-9007199254740991', '1.2.3-9007199254740993',
  // 全角数字・アラビア数字（JS の \d は 0-9 だけ）
  '１.２.３', '1.2.٣', '1.2.3-٣', '1.2.3-beta.٣',
  // 長さの境界（256 文字まで）
  '1.2.3-' + 'a'.repeat(250), '1.2.3-' + 'a'.repeat(251),
  // 長さは UTF-16 の単位で数える（U+3000 は UTF-16 で1単位、UTF-8 で3バイト）
  '　'.repeat(100) + '1.2.3', '　'.repeat(252) + '1.2.3',
]

// 大小比較の境界になるバージョン。総当たり（両方向）で比べる
const EDGE_COMPARE_VERSIONS = [
  '1.2.3-alpha', '1.2.3-alpha.1', '1.2.3-alpha.1.2', '1.2.3-alpha.beta',
  '1.2.3-beta', '1.2.3-beta.2', '1.2.3-beta.11', '1.2.3-rc.1',
  '1.2.3-0', '1.2.3-1', '1.2.3-01', '1.2.3-a1', '1.2.3-1a',
  '1.2.3-9007199254740990', '1.2.3-9007199254740993', '1.2.3-9007199254740994',
  '1.2.3-99999999999999999999',
  '1.2.3', '1.2.3+build', 'v1.2.3', '1.2.4',
]

// 1つの条件（Comparator）として読む入力。範囲を展開したあとに出てくる条件は、
// ranges.json の入力から自動で集めるので、ここには境界のケースだけを書く
const EDGE_COMPARATORS = [
  '', '>=1.2.3', '>1.2.3', '<1.2.3', '<=1.2.3', '=1.2.3', '1.2.3',
  // 空白。U+FEFF は JS では空白、U+0085 は空白ではない
  '>= 1.2.3', ' >= 1.2.3 ', '>=\t1.2.3', '>=﻿1.2.3', '>=\u00851.2.3',
  // loose モード特有の書き方
  'v1.2.3', '=v1.2.3', '>=v1.2.3', '>= v 1.2.3', '>=1.2.3beta', '<1.2.3-0',
  // 条件1つとしては不正なもの（範囲としてなら読めるものを含む）
  '>', '>=', '<>1.2.3', '>==1.2.3', '=== 1.2.3', '~1.2.3', '^1.2.3', '1.2', '1.x', '*',
  'abc', '>=1.2.3 <2.0.0', '>=9007199254740992.0.0',
]

// 公式テストデータ（`module.exports = [...]` の JS ファイル）を読み込む。
// 中で使われている require は、semver の内部定数の読み込みだけに限定する。
async function loadOfficialFixture (name) {
  const url = `${OFFICIAL_FIXTURES_URL}/${name}.js`
  const res = await fetch(url)
  if (!res.ok) {
    throw new Error(`failed to fetch ${url}: HTTP ${res.status}`)
  }
  const source = await res.text()
  const fixtureModule = { exports: null }
  const fixtureRequire = (id) => {
    if (id === '../../internal/constants') {
      return constants
    }
    throw new Error(`unexpected require in ${name}.js: ${id}`)
  }
  new Function('module', 'exports', 'require', source)(
    fixtureModule, fixtureModule.exports, fixtureRequire)
  return fixtureModule.exports
}

// 各行の index 番目のうち、文字列のものだけを取り出す（正規表現などの入力は対象外）
const stringsAt = (rows, index) =>
  rows.map((row) => row[index]).filter((value) => typeof value === 'string')

// 最初に出てきた順を保ったまま重複を除く
const uniqueBy = (items, key) => {
  const seen = new Set()
  return items.filter((item) => {
    const k = key(item)
    if (seen.has(k)) {
      return false
    }
    seen.add(k)
    return true
  })
}

const isRange = (input) => semver.validRange(input, LOOSE) !== null
const isVersion = (input) => semver.valid(input, LOOSE) !== null

// 差分を読みやすくするため、1件を1行で書く
function writeJson (name, entries) {
  const body = '[\n' + entries.map((entry) => '  ' + JSON.stringify(entry)).join(',\n') + '\n]\n'
  fs.writeFileSync(path.join(OUT_DIR, name), body)
  console.log(`wrote ${name} (${entries.length} entries)`)
}

async function main () {
  const include = await loadOfficialFixture('range-include')
  const exclude = await loadOfficialFixture('range-exclude')
  const rangeParse = await loadOfficialFixture('range-parse')
  const validVersions = await loadOfficialFixture('valid-versions')
  const invalidVersions = await loadOfficialFixture('invalid-versions')
  const comparisons = await loadOfficialFixture('comparisons')
  const equality = await loadOfficialFixture('equality')

  fs.mkdirSync(OUT_DIR, { recursive: true })

  // versions.json: バージョンとして読めるか。読めるなら正規化した形（読めなければ null）
  const versionInputs = uniqueBy([
    ...stringsAt(include, 1), ...stringsAt(exclude, 1),
    ...stringsAt(validVersions, 0), ...stringsAt(invalidVersions, 0),
    ...stringsAt(comparisons, 0), ...stringsAt(comparisons, 1),
    ...stringsAt(equality, 0), ...stringsAt(equality, 1),
    ...EDGE_VERSIONS, ...EDGE_VERSION_INPUTS, ...EDGE_COMPARE_VERSIONS,
  ], (input) => input)
  writeJson('versions.json', versionInputs.map((input) => ({
    input,
    normalized: semver.valid(input, LOOSE),
  })))

  // ranges.json: 範囲として読めるか。読めるなら正規化した形（読めなければ null）
  const rangeInputs = uniqueBy([
    ...stringsAt(include, 0), ...stringsAt(exclude, 0), ...stringsAt(rangeParse, 0),
    ...EDGE_RANGES,
  ], (input) => input)
  writeJson('ranges.json', rangeInputs.map((input) => ({
    input,
    normalized: semver.validRange(input, LOOSE),
  })))

  // comparators.json: 1つの条件として読めるか、読めたら正規化した形（value）と、
  // 境界のバージョンのうちどれを満たすか（matches）、満たさないか（rejects）
  const comparatorVersions = uniqueBy([...EDGE_VERSIONS, ...EDGE_COMPARE_VERSIONS], (v) => v)
  const comparatorInputs = uniqueBy([
    ...EDGE_COMPARATORS,
    ...rangeInputs.filter(isRange)
      .flatMap((range) => new semver.Range(range, LOOSE).set.flat().map((c) => c.value)),
  ], (input) => input)
  writeJson('comparators.json', comparatorInputs.map((input) => {
    let comparator
    try {
      comparator = new Comparator(input, LOOSE)
    } catch {
      return { input, value: null }
    }
    return {
      input,
      value: comparator.value,
      matches: comparatorVersions.filter((version) => comparator.test(version)),
      rejects: comparatorVersions.filter((version) => !comparator.test(version)),
    }
  }))

  // compare.json: 2つのバージョンの大小（a < b なら -1、同じなら 0、a > b なら 1）。
  // 公式データは「1つ目の方が大きい（または等しい）」組だけなので、逆向きの組も加える
  const comparePairs = [
    ...[...comparisons, ...equality].flatMap(([a, b]) => [[a, b], [b, a]]),
    ...EDGE_COMPARE_VERSIONS.flatMap((a) => EDGE_COMPARE_VERSIONS.map((b) => [a, b])),
  ]
    .filter(([a, b]) => typeof a === 'string' && typeof b === 'string')
    .filter(([a, b]) => isVersion(a) && isVersion(b))
  writeJson('compare.json', uniqueBy(
    comparePairs.map(([a, b]) => ({ a, b, expected: semver.compare(a, b, LOOSE) })),
    (entry) => `${entry.a}\u0000${entry.b}`,
  ))

  // satisfies.json: バージョンが範囲を満たすか（範囲・バージョンとも読めるものだけ）
  const satisfiesPairs = [
    ...[...include, ...exclude].map(([range, version]) => [range, version]),
    ...EDGE_RANGES.flatMap((range) => EDGE_VERSIONS.map((version) => [range, version])),
  ]
    .filter(([range, version]) => typeof range === 'string' && typeof version === 'string')
    .filter(([range, version]) => isRange(range) && isVersion(version))
  writeJson('satisfies.json', uniqueBy(
    satisfiesPairs.map(([range, version]) => ({
      range,
      version,
      expected: semver.satisfies(version, range, LOOSE),
    })),
    (entry) => `${entry.range}\u0000${entry.version}`,
  ))
}

main().catch((error) => {
  console.error(error)
  process.exitCode = 1
})
