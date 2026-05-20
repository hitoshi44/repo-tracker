# memo

gitlab API を使って、ci.yml とか package.json, pom.xml を track して、検索機能とかを提供する静的ウェブサイトを構築したい。
定期的に local で fetcher うごかして、html, json とかを s3 におく。

詳細設計は [DESIGN.md](./DESIGN.md)。

## 進捗

### 動いているもの

- Rust の fetcher (`cargo run` で起動)
- ハードコードした 2 repo (`gitlab-org/cli`, `gitlab-org/gitlab-svgs`) を順に叩いて、以下を書き出す:
  - `out/meta.json` — `fetched_at`, `repo_count`
  - `out/repos.json` — `Vec<Repository>`（files の path / kind 付き）
  - `out/files/<repo_id>/<path>.json` — `TrackedFile`（raw + blob_sha + size、`parsed` は空）
- 公開 repo なら `GITLAB_TOKEN` なしでも fetch できる
- 1 fetch ごとに 1 秒 sleep

### 残り

- パーサ (package.json / pom.xml / .gitlab-ci.yml の parsed)
- `ci-raws.json`（.gitlab-ci.yml の raw バンドル、全文検索用）
- config (yaml) 読み込み — 現状はハードコード配列
- S3 アップロード
- 静的サイト (index.html / app.js / style.css)
- `.gitignore`（`target/` と `out/` を入れる）

### 動作確認のヒント

- `gitlab-org/gitlab-foss` は root が 100 件超のため per_page=100 固定 (ページング無し、DESIGN 通り) で末尾が落ちる。動作確認には root が小さい repo（`gitlab-org/gitlab-svgs` など）を使う。

### 決まっていること

- 実装言語は Rust (blocking reqwest + serde)
- 認証は `GITLAB_TOKEN` 環境変数（無いと公開 repo のみアクセス可能）
- parsed の役割: フィールド単位の構造化検索 + ファイル詳細画面の表示。全文 grep は `ci-raws.json` 側に分担。
