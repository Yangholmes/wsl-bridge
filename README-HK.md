# WSL Bridge

<p align="center">
  <img src="src-tauri/app/icons/128x128.png" alt="WSL Bridge Logo" width="128" height="128">
</p>

<p align="center">
  <strong>面向 Windows 的 WSL / Hyper-V 網路代理、Hosts 管理與 AI 集成桌面工具</strong>
</p>

<p align="center">
  <a href="https://apps.microsoft.com/detail/9N3B2WPJ0BLQ">
    <img src="https://get.microsoft.com/images/en-us%20dark.svg" alt="Get from Microsoft Store" width="200">
  </a>
</p>

[English](README.md) | [简体中文](README-CN.md) | 繁體中文 | [日本語](README-JA.md)

---

## 取得方式

### Microsoft Store 「支持創作人」

可從 Microsoft Store 安裝，享受自動更新與 Windows 原生集成體驗：

**[→ 前往 Microsoft Store 下載](https://apps.microsoft.com/detail/9N3B2WPJ0BLQ)**

### GitHub Releases

也可從 GitHub Releases 下載獨立安裝包：

**[→ 前往 GitHub Releases](https://github.com/yangholmes/wsl-bridge/releases)**

提供格式：

- `MSI` 安裝包
- `NSIS` 便攜包

---

## 專案定位

WSL Bridge 是一款面向 Windows 10/11 的桌面應用，用於統一管理以下幾類本地開發服務暴露與訪問場景：

- `WSL`
- `Hyper-V`
- `Static host:port`

目前已不再只是「連接埠轉發工具」，而是擴展為包含反向代理、Hosts 管理、流量監控和 AI 集成能力的完整工作台。

---

## 目前模組

### Proxy

獨立的 `Proxy` 模組用於承載現代 HTTP 系流量分發與反向代理能力。

主要能力：

- Listener / Route / Upstream 三層拓撲模型
- HTTP / HTTPS Listener
- TLS 終止
- 使用者手動上傳證書
- 本地 CA 生成證書並用於 Listener
- 反向代理到 `WSL`、`Hyper-V`、`Static` 上游
- URL 級上游目標
- Path Prefix 改寫
- WebSocket 支援
- gRPC / gRPCS 第一版透傳支援
- `server_name` 分流與通配符匹配
- 按優先級匹配單條 Route
- 基於 PixiJS 的 Proxy 拓撲畫布：支援搜尋、縮放、平移、上下文選單、右側詳情面板

### Hosts

獨立的 `Hosts` 模組用於管理結構化的本地域名覆寫配置。

主要能力：

- 結構化 Hosts 分組，持久化到 SQLite
- 任意時刻僅一套「目前生效分組」寫入系統 `hosts`
- 表格式記錄編輯，支援 IPv4 / IPv6
- 分組複製、重新命名、刪除、導入、導出
- 首次使用時自動從系統 `hosts` 檔案導入到 `default`
- 導入導出使用系統原生檔案選擇器
- 實際寫入系統 `hosts` 需要管理員權限，但頁面不會隱藏，而是提供明確提示

### Rules

`Rules` 現已進入 legacy 模式。

目前定位：

- 繼續保留歷史規則的查看、修改、啟停、刪除
- 新建規則僅允許：
  - `udp_fwd`
  - `socks5_proxy`
- 舊的 `tcp_fwd` 與 `http_proxy` 可遷移到 `Proxy`

### Dashboard / 流量監控

首頁現在已同時聚合 `Rules` 與 `Proxy` 兩類運行與流量資料。

主要變化：

- `規則流量監控` 已升級為統一的 `流量監控`
- 可混合展示 Legacy Rules 與 Proxy Upstream 曲線
- 應用狀態、規則狀態、風險提示都已納入 Proxy 統計

### AI 集成

獨立的 `AI 集成` 模組集中管理 MCP、Skill 安裝、能力暴露和診斷。

主要能力：

- 內置 MCP 服務
- 面向 AI 的唯讀狀態資源：Proxy / Hosts / Rules / Traffic / Logs
- 基於結構化 `ConfigPatch` 的：
  - `dry-run`
  - 事務式 `apply`
  - 配置校驗
  - 連通性測試
- Agent Skill 預覽、安裝、卸載
- Skill 安裝前自動檢查並補齊全域 MCP 客戶端配置
- AI 相關審計日誌

目前已對齊的 Agent 目標包括：

- Claude Code
- Codex
- Cursor
- Copilot
- OpenCode
- 通用 `.agents`

---

## 核心能力總覽

目前 WSL Bridge 已覆蓋：

- TCP / UDP 轉發
- SOCKS5 代理
- 面向 HTTP 系開發服務的反向代理
- WSL / Hyper-V 動態目標發現與自動重綁
- 多網卡監聽 / 綁定模式
- 防火牆配置集成
- 結構化 Hosts 管理
- WSL / Hyper-V / NIC 拓撲掃描
- 審計日誌與運行日誌
- AI 輔助讀取、規劃、預檢與受控寫入

---

## MCP / AI API 模型

目前 AI 介面不再沿用「每個按鈕一個 MCP Tool」的膨脹式設計，而是採用以下模型：

- 少量 MCP Tools
- 豐富 MCP Resources
- 結構化 ConfigPatch
- validate / dry-run / test 閉環
- `wsl-bridge-operator` Skill

代表性 MCP Resources：

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

代表性 MCP Tools：

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

## 技術棧

### 前端

- [Solid.js](https://www.solidjs.com/)
- [TanStack Router](https://tanstack.com/router)
- [TanStack Query](https://tanstack.com/query)
- [TanStack Table](https://tanstack.com/table)
- [Kobalte](https://kobalte.dev/)
- [PixiJS](https://pixijs.com/)：用於 Proxy 拓撲畫布

### 後端

- [Tauri 2](https://v2.tauri.app/)
- [Rust](https://www.rust-lang.org/)
- [Tokio](https://tokio.rs/)
- [SQLite](https://sqlite.org/)

### 工具鏈

- `Vite`
- `pnpm`
- `Cargo`

---

## 快速開始

### 系統要求

- Windows 10 `22H2+` 或 Windows 11
- 如需使用 WSL 目標，請先安裝 WSL
- 如需使用 Hyper-V 目標，請先啟用 Hyper-V
- 如需完整 Hosts / 防火牆 / 運行時能力，建議以管理員身份運行

### 首次使用

1. 從 Microsoft Store 或 GitHub Releases 安裝應用。
2. 打開 `Topology`，掃描目前網路環境。
3. 如需 HTTP / HTTPS 反向代理，請進入 `Proxy`。
4. 如需 Legacy UDP 轉發或 SOCKS5，請進入 `Rules`。
5. 如需本地域名覆寫，請進入 `Hosts`。
6. 如需 AI 輔助配置，請進入 `AI 集成` 查看本地 MCP 服務狀態。

### 典型使用場景

#### 將 WSL Web 服務暴露到局域網

1. 打開 `Proxy`。
2. 建立一個監聽 `0.0.0.0:<port>` 的 `Listener`。
3. 建立 `Route`，按需填寫 `server_name` 與路徑前綴。
4. 建立指向 WSL 發行版與目標連接埠的 `Upstream`。
5. 使用內置連通性測試，或在其他局域網裝置訪問該地址。

#### 管理多套本地 Hosts 預設

1. 打開 `Hosts`。
2. 建立或導入多個分組。
3. 在記錄編輯彈窗中維護每組 Hosts 記錄。
4. 打開目標分組的開關，將其整體寫入系統 `hosts` 檔案。

#### 安裝 AI 集成

1. 打開 `AI 集成`。
2. 確認本地 MCP 服務狀態正常。
3. 選擇目標 Agent。
4. 查看 Skill 安裝預覽。
5. 如有需要，先安裝全域 MCP 客戶端配置，再安裝全域或專案級 Skill。

---

## 開發

```powershell
# 安裝依賴
pnpm install

# 啟動前端 + Tauri 開發環境
pnpm tauri dev

# 類型檢查
pnpm typecheck

# 構建前端
pnpm build

# 構建桌面應用
pnpm tauri build
```

開發前建議先閱讀：

- [docs/wsl-bridge-design.md](docs/wsl-bridge-design.md)
- [docs/wsl-bridge-uiux-design.md](docs/wsl-bridge-uiux-design.md)
- [docs/开发日志.md](docs/开发日志.md)

---

## 貢獻

歡迎提交 Issue 和 Pull Request。

提交問題時，建議同時說明：

- Windows 版本
- WSL / Hyper-V 環境資訊
- 重現步驟
- 問題發生在 `Rules`、`Proxy`、`Hosts` 還是 `AI 集成`

---

## 授權

[MIT License](LICENSE)
