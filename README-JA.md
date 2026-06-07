# WSL Bridge

<p align="center">
  <img src="src-tauri/app/icons/128x128.png" alt="WSL Bridge Logo" width="128" height="128">
</p>

<p align="center">
  <strong>Windows 向けの WSL / Hyper-V ネットワーク公開、Hosts 管理、AI 連携デスクトップツール</strong>
</p>

<p align="center">
  <a href="https://apps.microsoft.com/detail/9N3B2WPJ0BLQ">
    <img src="https://get.microsoft.com/images/en-us%20dark.svg" alt="Get from Microsoft Store" width="200">
  </a>
</p>

[English](README.md) | [简体中文](README-CN.md) | [繁體中文](README-HK.md) | 日本語

---

## 入手方法

### Microsoft Store 「作者を応援する」

Microsoft Store からインストールすると、自動更新と Windows ネイティブ統合を利用できます。

**[→ Microsoft Store からダウンロード](https://apps.microsoft.com/detail/9N3B2WPJ0BLQ)**

### GitHub Releases

GitHub Releases から単体インストーラーを入手することもできます。

**[→ GitHub Releases を開く](https://github.com/yangholmes/wsl-bridge/releases)**

提供形式:

- `MSI` インストーラー
- `NSIS` ポータブルパッケージ

---

## このアプリについて

WSL Bridge は、Windows 10/11 上で次のようなローカル開発サービス公開とアクセス経路を一元管理するためのデスクトップアプリです。

- `WSL`
- `Hyper-V`
- `Static host:port`

現在は単なるポート転送ツールではなく、リバースプロキシ、Hosts 管理、トラフィック監視、AI 連携ワークスペースをまとめた統合ツールになっています。

---

## 現在のモジュール

### Proxy

独立した `Proxy` モジュールは、現代的な HTTP 系トラフィック分配とリバースプロキシ運用を担当します。

主な機能:

- Listener / Route / Upstream の 3 層トポロジーモデル
- HTTP / HTTPS Listener
- TLS 終端
- ユーザーによる証明書アップロード
- ローカル CA 生成証明書の Listener 利用
- `WSL`、`Hyper-V`、`Static` 上流へのリバースプロキシ
- URL レベルの上流ターゲット指定
- Path Prefix 書き換え
- WebSocket 対応
- gRPC / gRPCS の初版透過転送
- `server_name` ベースの分流とワイルドカード一致
- 優先度に基づく単一 Route マッチング
- PixiJS ベースの Proxy トポロジーキャンバス
  - 検索
  - ズーム
  - パン
  - コンテキストメニュー
  - 右側詳細パネル

### Hosts

独立した `Hosts` モジュールは、構造化されたローカルドメイン上書き設定を管理します。

主な機能:

- SQLite に保存される構造化 Hosts グループ
- 常に 1 つの「現在有効なグループ」だけをシステム `hosts` に書き込み
- IPv4 / IPv6 対応の表形式レコード編集
- グループ複製、名称変更、削除、インポート、エクスポート
- 初回利用時に現在のシステム `hosts` を `default` に取り込み
- インポート / エクスポートに OS ネイティブのファイルピッカーを使用
- 実際のシステム `hosts` 書き込みには管理者権限が必要。ただしタブは非表示にせず、ガイダンスを表示

### Rules

`Rules` は現在 legacy モジュールです。

現在の位置づけ:

- 既存ルールの表示、編集、有効化 / 無効化、削除は継続
- 新規作成できるルールは次のみ:
  - `udp_fwd`
  - `socks5_proxy`
- 旧 `tcp_fwd` と `http_proxy` は `Proxy` へ移行可能

### Dashboard / トラフィック監視

ダッシュボードでは `Rules` と `Proxy` の両方の実行状態とトラフィックを集約して表示します。

主な内容:

- 統合トラフィック監視
- `Legacy Rules + Proxy Upstream` の混合表示
- アプリ状態、ルール状態、リスク表示に Proxy 統計を反映

### AI Integration

独立した `AI 集成` モジュールでは、MCP、Skill インストール、公開能力、診断を一元管理します。

主な機能:

- 内蔵 MCP サーバー
- Proxy / Hosts / Rules / Traffic / Logs の AI 向け読み取り専用状態リソース
- 構造化 `ConfigPatch` に基づく:
  - `dry-run`
  - トランザクション型 `apply`
  - 設定検証
  - 接続確認テスト
- Agent Skill のプレビュー、インストール、アンインストール
- Skill インストール前のグローバル MCP クライアント設定チェックと自動補完
- AI 関連の監査ログ

現在対応している Agent ターゲット:

- Claude Code
- Codex
- Cursor
- Copilot
- OpenCode
- 汎用 `.agents`

---

## 主な機能一覧

WSL Bridge は現在、次の領域をカバーしています。

- TCP / UDP 転送
- SOCKS5 プロキシ
- HTTP 系開発サービス向けリバースプロキシ
- WSL / Hyper-V ターゲットの動的検出と自動再バインド
- 複数 NIC の待ち受け / バインドモード
- ファイアウォール設定連携
- 構造化 Hosts 管理
- WSL / Hyper-V / NIC トポロジー検出
- 監査ログと実行ログ
- AI による参照、計画、事前検証、制御付き適用

---

## MCP / AI API モデル

現在の AI インターフェースは、「ボタン 1 つにつき MCP Tool 1 つ」という形ではありません。代わりに次のモデルを採用しています。

- 少数の MCP Tools
- 豊富な MCP Resources
- 構造化 ConfigPatch
- validate / dry-run / test のループ
- `wsl-bridge-operator` Skill

代表的な MCP Resources:

- `wsl-bridge://ai-guide`
- `wsl-bridge://capabilities`
- `wsl-bridge://state/summary`
- `wsl-bridge://state/proxy`
- `wsl-bridge://state/hosts`
- `wsl-bridge://state/rules`
- `wsl-bridge://state/traffic`
- `wsl-bridge://logs/recent`
- `wsl-bridge://schemas/config-patch`
- `wsl-bridge://schemas/state`

代表的な MCP Tools:

- `inspect_app`
- `validate_config`
- `apply_config_patch`
- `test_connectivity`
- `export_config`
- `import_config`
- `list_agent_targets`
- `install_agent_skill`
- `uninstall_agent_skill`

---

## 技術スタック

### フロントエンド

- [Solid.js](https://www.solidjs.com/)
- [TanStack Router](https://tanstack.com/router)
- [TanStack Query](https://tanstack.com/query)
- [TanStack Table](https://tanstack.com/table)
- [Kobalte](https://kobalte.dev/)
- [PixiJS](https://pixijs.com/) - Proxy トポロジーキャンバス用

### バックエンド

- [Tauri 2](https://v2.tauri.app/)
- [Rust](https://www.rust-lang.org/)
- [Tokio](https://tokio.rs/)
- [SQLite](https://sqlite.org/)

### ツールチェーン

- `Vite`
- `pnpm`
- `Cargo`

---

## クイックスタート

### 動作要件

- Windows 10 `22H2+` または Windows 11
- WSL ターゲットを使う場合は WSL を事前にインストール
- Hyper-V ターゲットを使う場合は Hyper-V を有効化
- Hosts / ファイアウォール / 実行時機能をフルに使うには管理者権限推奨

### 初回利用

1. Microsoft Store または GitHub Releases からアプリをインストールします。
2. `Topology` を開き、現在のネットワーク環境をスキャンします。
3. HTTP / HTTPS のリバースプロキシが必要なら `Proxy` を開きます。
4. Legacy UDP 転送や SOCKS5 が必要なら `Rules` を開きます。
5. ローカルドメイン上書きが必要なら `Hosts` を開きます。
6. AI 補助操作を使いたい場合は `AI 集成` を開き、ローカル MCP サービス状態を確認します。

### 代表的な利用シナリオ

#### WSL の Web サービスを LAN に公開する

1. `Proxy` を開きます。
2. `0.0.0.0:<port>` を待ち受ける `Listener` を作成します。
3. 必要に応じて `server_name` とパスプレフィックスを設定した `Route` を作成します。
4. WSL ディストリビューションと対象ポートを指す `Upstream` を作成します。
5. 内蔵の接続確認テストを使うか、別デバイスから実際にアクセスします。

#### 複数の Hosts プリセットを管理する

1. `Hosts` を開きます。
2. 複数のグループを作成またはインポートします。
3. レコード編集モーダルで各グループの Hosts レコードを管理します。
4. 対象グループのスイッチを有効にして、システム `hosts` に書き込みます。

#### AI 連携をセットアップする

1. `AI 集成` を開きます。
2. ローカル MCP サービス状態を確認します。
3. 対象 Agent を選択します。
4. Skill インストールプレビューを確認します。
5. 必要に応じて先にグローバル MCP クライアント設定をインストールし、その後グローバルまたはプロジェクト単位で Skill をインストールします。

---

## 開発

```powershell
# 依存関係をインストール
pnpm install

# フロントエンド + Tauri 開発環境を起動
pnpm tauri dev

# 型チェック
pnpm typecheck

# フロントエンドをビルド
pnpm build

# デスクトップアプリをビルド
pnpm tauri build
```

開発前に読むとよい資料:

- [docs/wsl-bridge-design.md](docs/wsl-bridge-design.md)
- [docs/wsl-bridge-uiux-design.md](docs/wsl-bridge-uiux-design.md)
- [docs/开发日志.md](docs/开发日志.md)

---

## コントリビュート

Issue と Pull Request を歓迎します。

問題報告の際は、可能であれば次も含めてください。

- Windows バージョン
- WSL / Hyper-V 環境情報
- 再現手順
- 問題が `Rules`、`Proxy`、`Hosts`、`AI 集成` のどこで発生したか

---

## ライセンス

[MIT License](LICENSE)
