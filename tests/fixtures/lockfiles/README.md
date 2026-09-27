# ロックファイルのテストデータ

peerdoctor がロックファイルを読み、peer の提供元を配置どおりに解決できるかを確かめるためのデータ。

## 作り方

`scripts/lockfile-fixtures/generate.js` が、手で定義したパッケージを返すレジストリをローカル
（`http://localhost:4873`）に立て、シナリオごとに `npm install --package-lock-only` を実行して作る。
配置（どのパッケージをどこにネストするか）は npm 自身が決めている。

レジストリを立てるのは、`file:` で入れると `resolved` が `file:` になり、peerdoctor が未対応の構成
として拒否してしまうため。

```bash
node scripts/lockfile-fixtures/generate.js
```

npm 11 で作成した。ポート 4873 が空いている必要がある。生成結果は決定的で、何度作り直しても同じ内容になる。

## シナリオ

| ディレクトリ | 構成 | 確かめたいこと |
|---|---|---|
| `basic/` | `next@15.3.0`、`react@18.2.0` と、`next` を peer に持つ `plugin-a`・`plugin-b`・`plugin-c`（`plugin-c` は `react` を optional な peer に持つ） | 基本の読み取り |
| `basic-v2/` | `basic` と同じ構成を lockfileVersion 2 で | v2 と v3 で同じ結果になるか |
| `nested/` | ルートに `next@15.3.0`。`legacy-host` の下に `next@14.2.0` と、`next ^14` を peer に持つ `plugin-old` がネストする | ネストした同名パッケージがあっても、ルートの `next` を指す peer だけを拾えるか |
| `scoped/` | `@acme/next-plugin` が `next` を peer に持つ | スコープ付きの名前のパスを扱えるか |
| `unsupported-workspaces/` | npm workspaces（`packages/member`） | 未対応の構成として拒否するか |
| `unsupported-link/` | `file:./local-lib`（ディレクトリへのリンク） | 同上 |
| `unsupported-file/` | `file:./local-tgz-1.0.0.tgz`（ローカルの tarball） | 同上 |
| `unsupported-alias/` | `npm:plugin-a@2.1.0`（エイリアス） | 同上 |
| `unsupported-bundled/` | `bundleDependencies` を持つ `bundle-host` | 同上 |
| `overrides/` | package.json の `overrides` | 警告して続行するか（overrides はロックファイルには記録されない） |
| `unverified/` | `plugin-tag` が peer に `next: "latest"`（範囲ではなく dist-tag）を持つ。npm 自身が peer の衝突で止まるので `--legacy-peer-deps` で作成 | 範囲として読めない peer を unverified として報告するか |

git 依存のロックファイルは、作るのに git サーバーが要るので、テストの中で JSON を手書きしている。

パッケージの中身（バージョン、依存、peer）は `generate.js` の `PACKAGES` に、各シナリオの依存は `SCENARIOS` にある。
