# AGENTS.md - AI 助手开发指南

## 项目概述

- **项目名称**: wsl-bridge
- **项目类型**: Tauri 2 桌面应用（Windows）
- **技术栈**:
  - 前端: Solid.js + TanStack Router + TanStack Query + TanStack Table
  - 后端: Rust (Tauri 2)
  - 包管理: pnpm
  - 语言: TypeScript

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

# 预览构建结果
pnpm tauri preview
```

## 代码规范

### 缩进

- **严格使用 2 空格缩进**，禁止使用 tab 或其他缩进方式
- 配置编辑器: `indentSize: 2`, `indentStyle: space`

### 目录结构

```
src/                      # 前端代码（Solid.js）
├── features/             # 页面功能模块
│   ├── dashboard/
│   ├── rules/
│   ├── runtime/
│   ├── topology/
│   ├── logs/
│   └── settings/
├── lib/                  # 工具函数与类型
├── i18n/                 # 国际化
├── assets/               # 静态资源
├── router.tsx            # 路由配置
├── main.tsx              # 入口文件
└── styles.css            # 全局样式

src-tauri/                # Tauri 后端代码（Rust）
├── app/                  # Tauri 应用主目录
│   ├── src/              # 应用入口代码
│   ├── Cargo.toml       # 应用依赖
│   ├── tauri.conf.json  # Tauri 配置
│   └── capabilities/    # 权限配置
└── crates/               # 共享 crate
    ├── core/             # 核心业务逻辑
    └── shared/           # 共享 DTO/类型
```

### Tauri App 目录规范

严格遵循 Tauri 2 官方目录结构：

- `src-tauri/app/` - 应用主目录
- `src-tauri/app/src/` - Rust 源代码
- `src-tauri/app/Cargo.toml` - 应用级依赖
- `src-tauri/app/tauri.conf.json` - Tauri 配置
- `src-tauri/crates/` - 独立的 crate 包

## 文档参考

**重要**: 进行开发前，务必先阅读 `docs/` 目录下的设计文档：

- `docs/wsl-bridge-design.md` - 总体架构与技术设计
- `docs/wsl-bridge-uiux-design.md` - UI/UX 设计规范
- `docs/dashboard-开发计划.md` - 开发计划
- `docs/开发日志.md` - 开发日志

## 第三方库文档

**重要**: 禁止直接阅读 `node_modules/` 目录下的源代码！

如需了解第三方库的使用方法，请使用 **Context7** 进行搜索：

## Tauri Command 开发

在 `src-tauri/app/src/` 下创建或修改命令：

1. 在 `commands.rs` 中定义新命令
2. 在 `main.rs` 中注册命令
3. 在前端 `src/lib/bridge.ts` 中调用

### 前端调用示例

```typescript
import { invokeBridge } from "./lib/bridge";

const result = await invokeBridge<ReturnType>("command_name", {
  param: "value",
});
```

## 状态管理

- 使用 **TanStack Query** 进行服务端状态管理
- 使用 **Solid.js Store** 进行客户端 UI 状态管理

## 国际化

- 国际化文件位于 `src/i18n/locales/`
- 支持: `zh-CN`, `zh-HK`, `en-US`, `ja-JP`
- 使用 `useI18n` hook 获取翻译

## 注意事项

1. **不要读取 node_modules** - 使用 Context7 查询文档
2. **保持 2 空格缩进** - 检查编辑器配置
3. **遵循 Tauri 目录规范** - 使用 `src-tauri/app/` 结构
4. **先读文档** - 开发前查阅 `docs/` 目录
5. **使用 pnpm** - 不要使用 npm 或 yarn
6. **记录开发日志** - 每一个功能开发都需要记录开发日志
7. **禁止使用 emoji 作为图标** - 项目中不应使用 emoji（如 ✓、▾ 等）作为 UI 图标，应使用 CSS 样式或 SVG 图标替代

## 代码提交

提交前确保:

- [ ] `pnpm typecheck` 无错误
- [ ] 新增功能已测试
