# CLAUDE.md

このファイルは Claude Code がこのリポジトリを扱う際のガイダンスを提供する。

## Project Overview

`kakera`(欠片）は、**ターゲット画像 1 枚と画像フォルダを入力に、フォルダ内の画像をタイルとして敷き詰めたモザイクアートを生成する**ツール。

コアロジックは Rust で実装し、**CLI / Cloudflare Pages 上の Web アプリ / Tauri 2.0 デスクトップアプリ**の 3 形態で同一コアを共有する。狙いは「画像処理の本体を一度だけ書き、各プラットフォームは I/O とUI のアダプタに徹する」こと。Web 版は WASM 経由でコアを呼ぶ。

ソース画像群は事前に色特徴をインデックス化して再利用する設計（`gather` でインデックス構築 → `build` で組み立て、というテーマに沿った二段構え)。

> **現状はスケルトン段階。** Rust 3 クレートは `add()` スタブ関数のみ、`apps/web/` と `apps/desktop/` はテンプレート直後の状態（モザイク UI 未実装）、`src-tauri/` は雛形コマンドのみで `kakera-core` 未連携、git コミットは未作成。モザイク生成ロジックは未実装。将来計画は各 README に譲り、ここには**現存するファイル・動くコマンドのみ**を記載する。

## Architecture

Cargo workspace（ルート `Cargo.toml`、members = `crates/*`）+ `apps/` 配下のフロント。

```
kakera/
├── Cargo.toml            # workspace root (members = crates/* のみ)
├── crates/
│   ├── kakera-core/      # 純Rustコア。I/O非依存。dep: image
│   ├── kakera-cli/       # CLIバイナリ。dep: kakera-core
│   └── kakera-wasm/      # wasm-bindgenラッパー(予定)。dep: kakera-core
└── apps/
    ├── web/              # Next.js 16 + React 19 + Chakra UI 3 (pnpm)
    └── desktop/          # Tauri 2 + Next.js 16 フロント (Panda CSS)
        └── src-tauri/    # Tauri Rust 側。独立 Cargo crate (workspace外)
```

依存方向は一方向: **`kakera-core` ← `kakera-cli` / `kakera-wasm` ← `apps/`**。逆参照禁止。

- `kakera-core`: プラットフォーム非依存。I/O を持たず「RGBA buffer in → RGBA buffer out」を基本インターフェースとする。ファイルシステム・ネットワーク・UI に触れない。
- `kakera-cli` / `kakera-wasm` / `apps/desktop/src-tauri`: ファイル取得・保存・UI を吸収するアダプタ層。各エントリポイント固有の処理はここに閉じる。
- `apps/web`: Next.js（App Router、`src/app/`）。WASM をロードしてコアを呼ぶ想定。
- `apps/desktop`: Tauri 2 アプリ。フロントは独自の Next.js（App Router、Panda CSS）で web とは別ツリー。Rust 側 `src-tauri/`（crate 名 `app`、lib `app_lib`）から `kakera-core` を呼ぶ想定。
- **`src-tauri/Cargo.toml` はルート workspace に含まれない独立クレート**（root members = `crates/*`）。現状 `kakera-core` への path 依存はなく、deps は tauri / tauri-plugin-log / serde 系のみ。コア連携時は `src-tauri/Cargo.toml` に `kakera-core` の path 依存を足す。

ワークスペース依存（ルート `Cargo.toml` の `[workspace.dependencies]`）: `image = "0.25"`、`anyhow = "1"`。`rayon` / `wasm-bindgen` / `clap` 等はまだ未導入。

## Build & Run Commands

すべてリポジトリルートで実行（特記なき限り）。

### Rust (cli / core / wasm)

| コマンド | 役割 |
| --- | --- |
| `cargo build` | workspace 全クレートをビルド |
| `cargo test` | 全クレートのテスト（現状は `add()` のサンプルテストのみ） |
| `cargo run -p kakera-cli` | CLI 実行（現状 `Hello, world!` を出力するだけ） |
| `cargo build -p kakera-core` | コアのみビルド |

- `gather` / `build` / `preview` サブコマンドは**未実装**（設計上の予定）。
- WASM ビルド（`wasm-pack` / `wasm-bindgen` 等）は **TBD**: `kakera-wasm` はまだ `cdylib` 化も `wasm-bindgen` 依存もしていない。

### Web (`apps/web/`)

`apps/web/` で pnpm を使う（`pnpm-lock.yaml` あり）。

| コマンド | 役割 |
| --- | --- |
| `pnpm dev` | Next.js 開発サーバ（http://localhost:3000） |
| `pnpm build` | 本番ビルド（`next build`） |
| `pnpm start` | ビルド済みを起動 |
| `pnpm lint` | `oxlint --fix`（eslint ではない） |
| `pnpm format` | `oxfmt`（prettier ではない） |

- Cloudflare Pages 向けの static export（`next.config.ts` の `output: 'export'`）は**未設定**。デプロイ手順は **TBD**。

### Desktop (Tauri)

`apps/desktop/` で pnpm を使う。Tauri 2（`@tauri-apps/cli ^2.11.2`、`tauri 2.11.2`）。

| コマンド | 役割 |
| --- | --- |
| `pnpm prepare` | Panda CSS の codegen（`styled-system/` 生成）。初回/設定変更時 |
| `pnpm dev` | フロントのみ開発サーバ（http://localhost:3000） |
| `pnpm tauri dev` | Tauri デスクトップを開発起動（`beforeDevCommand: pnpm dev`） |
| `pnpm tauri build` | デスクトップアプリをバンドル（`beforeBuildCommand: pnpm build`） |
| `pnpm lint` / `pnpm format` | `oxlint --fix` / `oxfmt` |

- `tauri.conf.json` は `frontendDist: ../out` を期待するが、`next.config.ts` に `output: 'export'` が**未設定**。このため `pnpm tauri build` は `out/` 不在で失敗する見込み。static export 設定は **TBD**。
- `src-tauri/src/{main,lib.rs}` は雛形（`tauri::Builder` にコマンド登録なし、debug 時に `tauri-plugin-log` のみ）。`identifier` は `com.tauri.dev` のままで要変更。

## Coding Conventions

### Rust

- **`kakera-core` は I/O 非依存を厳守**: `std::fs` / ネットワーク / 環境変数 / UI を持ち込まない。入出力はバッファ（RGBA）と引数で受け渡す。ファイル読み書きは cli / wasm / tauri 側アダプタの責務。
- 各プラットフォーム固有処理はアダプタクレートに閉じ、`kakera-core` へ漏らさない。
- エラー型方針: **ライブラリである `kakera-core` は型付きエラー（`thiserror` 想定）**、**アダプタ（cli/wasm/tauri）は `anyhow`** で集約、を基本とする。現状 `thiserror` は未導入（`anyhow` はワークスペース宣言済みだが未使用）。
- 並列化は `kakera-core` 内で `rayon` を使う方針だが未導入。WASM での並列化は下記 Pitfalls 参照。
- edition は workspace 継承（2021）。新クレートも `version.workspace = true` / `edition.workspace = true` を踏襲。

### Web / Desktop フロント（共通）

- **この Next.js は 16.x。** 訓練データの Next.js とは API・規約・ファイル構成が異なる。コードを書く前に `node_modules/next/dist/docs/` の該当ガイドを参照し、deprecation 通知に従う（`apps/web/AGENTS.md` 参照。`apps/desktop` に AGENTS.md は無いが同じ注意が必要）。
- Linter/Formatter は **oxlint / oxfmt**（eslint/prettier ではない）。
- React Compiler 有効（`next.config.ts` の `reactCompiler: true`）。手動メモ化を足す前にコンパイラ前提か確認する。
- TS パスエイリアス `@/*` → `./src/*`。App Router（`src/app/`）。
- スタイリングが両者で異なる:
  - `apps/web`: Chakra UI v3 + `@emotion/react`。共通 provider は `src/components/ui/`。
  - `apps/desktop`: **Panda CSS**（`panda.config.ts` + 生成物 `styled-system/`）。`styled-system/` は `panda codegen` の生成物なので**手編集しない**。Chakra/emotion も依存にあるが styling の主体は Panda。
- web と desktop はフロントを共有していない（別 Next.js プロジェクト）。共通化する場合は別途設計が必要。

### ログ

- ログ方針は **TBD**（まだロギング基盤なし）。導入時に `kakera-core` は facade（`log` クレート等）に留め、出力先はアダプタ側で決める、を想定。

## Cross-Platform Pitfalls

- **WASM の並列化**: `kakera-core` でネイティブ `rayon` を使う場合、WASM では `wasm-bindgen-rayon` が必要で、かつ SharedArrayBuffer を有効化するため Cloudflare Pages 側で **COOP/COEP ヘッダ**（`Cross-Origin-Opener-Policy: same-origin` / `Cross-Origin-Embedder-Policy: require-corp`）の付与が要る。未設定だとマルチスレッド WASM が動かない。未実装。
- **画像デコード戦略の差**:
  - ネイティブ（cli）: `image` クレートでデコード/エンコード。
  - ブラウザ（web）: **`createImageBitmap` でデコードした RGBA を WASM に渡す**。WASM 内で `image` のデコーダを使うのは避ける（バイナリ肥大・性能劣化のため）。`kakera-core` の API は「デコード済み RGBA」前提で設計する。
- **フォルダ選択 UX のプラットフォーム差**: cli は引数でパス受領、web は File System Access API（または `<input webkitdirectory>`）、Tauri は `@tauri-apps/plugin-dialog` 等のネイティブダイアログ（未導入）。この差はすべてアダプタ層で吸収し、`kakera-core` には渡ってきた画像バッファ列のみ見せる。
- **Tauri の frontendDist**: `tauri.conf.json` は静的書き出し（`apps/desktop/out`）を前提とする。`pnpm tauri build` 前に Next.js の `output: 'export'` を設定しないとバンドルが失敗する（現状未設定）。dev（`pnpm tauri dev`）は `devUrl` 経由なので影響しない。

## Key Files

触ってよい / 注意するファイルの簡易マップ。

- `crates/kakera-core/src/lib.rs` — コア本体。**ここに I/O を入れない**。モザイクロジックの実装先。
- `crates/kakera-cli/src/main.rs` — CLI エントリ。引数解析・ファイル I/O はここ。
- `crates/kakera-wasm/src/lib.rs` — WASM ラッパー。`wasm-bindgen` エクスポートはここ。
- `Cargo.toml`（ルート）— workspace 共有依存。新規依存はまずここに足して各クレートで `.workspace = true`。
- `apps/web/AGENTS.md` / `apps/web/CLAUDE.md` — **web で作業する前に必読**（Next.js 16 の差分注意書き）。
- `apps/web/next.config.ts` / `apps/desktop/next.config.ts` — static export（`output: 'export'`）を入れるならここ（web=Cloudflare Pages 向け、desktop=Tauri バンドル向け。どちらも現状未設定）。
- `apps/web/src/app/page.tsx` / `apps/desktop/src/app/page.tsx` — まだ create-next-app のボイラープレート。UI 実装の起点。
- `apps/desktop/src-tauri/src/lib.rs` — Tauri エントリ（`run()`）。`#[tauri::command]` 登録・`kakera-core` 連携はここ。
- `apps/desktop/src-tauri/Cargo.toml` — Tauri Rust 依存。`kakera-core` 連携時に path 依存を足す先（workspace 外なので個別管理）。
- `apps/desktop/src-tauri/tauri.conf.json` — ウィンドウ/バンドル/`frontendDist` 設定。`identifier` 要変更。
- `apps/desktop/panda.config.ts` — Panda CSS 設定。変更後は `pnpm prepare`。
- 自動生成・触らない: `target/`、`apps/*/.next/`、`apps/*/node_modules/`、`apps/*/pnpm-lock.yaml`、`apps/desktop/styled-system/`（Panda 生成物）。
