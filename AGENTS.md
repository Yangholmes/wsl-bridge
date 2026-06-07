# AGENTS.md - AI 助手开发指南

## 项目概述

- **项目名称**: `wsl-bridge`
- **项目类型**: Tauri 2 桌面应用（Windows）
- **当前产品形态**:
  - Legacy `Rules` 转发管理
  - `Proxy` 反向代理工作台
  - `Hosts` 分组管理
  - `AI 集成` 工作台
  - `Dashboard / Traffic / Topology / Settings`
- **技术栈**:
  - 前端: Solid.js + TanStack Router + TanStack Query + TanStack Table + Kobalte + PixiJS
  - 后端: Rust (Tauri 2)
  - 包管理: pnpm
  - 语言: TypeScript + Rust

## 开发环境

### 常用命令

```bash
# 安装依赖
pnpm install

# 开发模式启动（前端 + Tauri）
pnpm tauri dev

# 构建前端
pnpm build

# 类型检查
pnpm typecheck

# 构建桌面应用
pnpm tauri build

# 预览构建结果
pnpm tauri preview
```

## 代码规范

### 缩进

- **严格使用 2 空格缩进**，禁止使用 tab 或其他缩进方式
- 配置编辑器: `indentSize: 2`, `indentStyle: space`

### 目录结构

```text
src/                          # 前端代码（Solid.js）
├── features/
│   ├── ai/                   # AI 集成
│   ├── dashboard/            # 首页 / 流量监控
│   ├── hosts/                # Hosts 管理
│   ├── proxy/                # Proxy 工作台与拓扑画布
│   ├── rules/                # Legacy Rules
│   ├── settings/
│   └── topology/
├── i18n/                     # 国际化
├── lib/                      # 公共组件、hooks、bridge、类型、样式
├── assets/                   # 静态资源
├── router.tsx                # 路由配置
├── main.tsx                  # 入口文件
└── styles.css                # 全局样式

src-tauri/
├── app/                      # Tauri 应用主目录
│   ├── src/
│   ├── Cargo.toml
│   ├── tauri.conf.json
│   └── capabilities/
└── crates/
    ├── core/                 # 核心业务逻辑
    └── shared/               # DTO / 类型定义
```

### 样式与组件

- **优先复用已有公共组件**，公共组件位于 `src/lib`
- 当前项目同时使用：
  - 公共全局样式文件，例如 `src/styles.css`、`src/lib/*.css`
  - Feature 级样式文件，例如 `src/features/**/**.css`
- **不要强行把现有代码迁移成 CSS Modules**
- 新增 UI 时优先延续当前项目的样式组织方式，而不是引入新的风格分叉

### Tauri App 目录规范

严格遵循 Tauri 2 官方目录结构：

- `src-tauri/app/` - 应用主目录
- `src-tauri/app/src/` - Rust 源代码
- `src-tauri/app/Cargo.toml` - 应用级依赖
- `src-tauri/app/tauri.conf.json` - Tauri 配置
- `src-tauri/crates/` - 独立 crate 包

## 产品与模块认知

开发前先明确当前模块边界：

- `Proxy`
  - Listener / Route / Upstream
  - HTTP / HTTPS / WebSocket / gRPC / gRPCS
  - PixiJS 拓扑画布
- `Hosts`
  - 分组化结构化 Hosts
  - 单一当前生效集写入系统 hosts
- `Rules`
  - **legacy 模块**
  - 新建仅允许 `udp_fwd` / `socks5_proxy`
  - `tcp_fwd` / `http_proxy` 迁移到 `Proxy`
- `AI 集成`
  - MCP 服务
  - MCP Resources / Tools
  - ConfigPatch dry-run / apply / validate / test
  - Agent Skill / MCP 客户端安装

如果需求涉及 HTTP 系代理能力，默认先看 `Proxy`，不要再向 `Rules` 里新增同类能力。

## 文档参考

**重要**: 开发前先阅读 `docs/` 目录中的设计与日志文档。

优先参考：

- `docs/wsl-bridge-design.md` - 总体架构与技术设计
- `docs/wsl-bridge-uiux-design.md` - UI / UX 设计规范
- `docs/phase3-ai-mcp-skill-supplement.md` - AI 集成补充方案
- `docs/proxy-canvas-ui-optimization-plan.md` - Proxy 画布方案
- `docs/开发日志.md` - 最新实现与收敛记录

如果是 README、文案、模块边界类更新，也要先对照 `README.md` 与 `README-CN.md` 当前状态。

## 第三方库文档

**重要**: 禁止直接阅读 `node_modules/` 源码来理解第三方库。

如需了解第三方库使用方法：

- 优先使用 **Context7**
- 若必须联网查阅，只看官方文档或一手资料

## Tauri Command / MCP 开发

### Tauri Command

在 `src-tauri/app/src/` 下创建或修改命令：

1. 在 `commands.rs` 或对应 command 模块中定义新命令
2. 在 `main.rs` 中注册命令
3. 在前端 `src/lib/bridge.ts` 或 feature API 中调用

前端调用示例：

```typescript
import { invokeBridge } from "./lib/bridge";

const result = await invokeBridge<ReturnType>("command_name", {
  param: "value",
});
```

### MCP / AI 集成

如果需求涉及 AI 集成：

- 优先复用现有 `mcp.rs` 里的资源、工具、ConfigPatch 闭环
- 不要回退成“一按钮一个 MCP tool”的膨胀式设计
- 新增写操作时优先考虑：
  - 是否应该进入 `ConfigPatch`
  - 是否需要 `dry-run`
  - 是否需要 `validate_config`
  - 是否需要 `test_connectivity`

## 状态管理

- 使用 **TanStack Query** 进行服务端状态管理
- 使用 **Solid.js Store** 进行客户端 UI 状态管理

## 国际化

- 国际化文件位于 `src/i18n/locales/`
- 支持:
  - `zh-CN`
  - `zh-HK`
  - `en-US`
  - `ja-JP`
- 使用 `useI18n` hook 获取翻译

### 国际化约束

- 新增用户可见文案时，必须同步补齐四种语言
- 不要硬编码最终展示文案
- 变更现有 UI 交互时，要检查相关词条是否已经失效或语义变化

## 测试与验收

### 基本要求

1. **所有改动都要自测**
2. **测试必须 100% 通过才能收敛**
3. 如果尝试超过 5 次仍未通过，必须把未通过项详细写入 `docs/开发日志.md`

### 常用验证

- 前端改动至少执行：
  - `pnpm typecheck`
- Rust / Tauri 改动至少执行：
  - `cargo check --manifest-path src-tauri/app/Cargo.toml`
- 如涉及 `src-tauri/app/src/mcp.rs`、AI 集成、Proxy / Hosts 事务逻辑，优先补或运行对应单测
- 如涉及 UI 复杂交互，可使用浏览器自动化或页面运行态检查

### 运行态调试

对于复杂 UI 问题：

- 不要只靠阅读 CSS 猜测
- 必要时直接检查真实 DOM 尺寸、computed style、运行态状态
- 对于 Proxy 画布，必要时检查：
  - `.content`
  - `.content-inner`
  - `.proxy-page-shell`
  - `.proxy-canvas-host`
  - `.proxy-canvas-stage`
  - `canvas`

## 开发日志

- **每一个功能开发都需要记录开发日志**
- 收敛前同步更新 `docs/开发日志.md`
- 开发日志要写清楚：
  - 改了什么
  - 原因是什么
  - 自测做了什么
  - 是否仍有债务或限制

## 图片识别与 UI 测试

- 纯 UI 视觉测试可以使用 `agent-browser` skill
- 如果需要理解截图内容，可使用具有多模态能力的模型协助判断

## 约束

1. **不要读取 `node_modules`**，使用 Context7 查询文档
2. **保持 2 空格缩进**
3. **遵循 Tauri 目录规范**
4. **开发前先读文档**
5. **使用 pnpm，不要使用 npm 或 yarn**
6. **同步记录开发日志**
7. **禁止使用 emoji 作为图标**
8. **新增 UI 优先复用公共组件**
9. **新增用户文案必须补齐多语言**
10. **测试通过后再收敛**

## 提交前检查

提交前确保：

- [ ] `pnpm typecheck` 无错误
- [ ] 新增功能已测试
- [ ] `docs/开发日志.md` 已同步
- [ ] 如涉及用户可见文案，四语言词条已补齐
