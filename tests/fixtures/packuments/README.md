# packument のテストデータ

レジストリが返すパッケージの情報（abbreviated packument。`Accept: application/vnd.npm.install-v1+json`
で取得できる形）。peerdoctor が候補の範囲を計算できるかを確かめるために使う。

`scripts/lockfile-fixtures/generate.js` がロックファイルを作るときに立てるローカルのレジストリが返したものを、
そのまま書き出している（`tarball` の URL は `http://localhost:4873/...` になる）。作り直し方は
`tests/fixtures/lockfiles/README.md` と同じ。スコープ付きの名前は `@acme/next-plugin.json` のように
ディレクトリに分けて置く。

候補の範囲を確かめるために、次のパッケージには版を多めに定義している（中身は `generate.js` の `PACKAGES`）。

| パッケージ | 確かめたいこと |
|---|---|
| `plugin-a` | 範囲の上限がある（4.x で next 16 を外す）、途中に deprecated な版がある、prerelease の版がある |
| `plugin-c` | どの版も next 16 を受け入れない（dead end） |
| `plugin-flaky` | 受け入れる版が飛び飛び（非連続） |
| `@acme/next-plugin` | スコープ付きの名前、範囲の上限がない、peer の宣言が消えた版がある |
