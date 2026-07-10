# Clarity 数据埋点方案

## 1. 文档信息

- 文档版本：`v1.0`
- 创建时间：`2026-04-29`
- 目标读者：研发

## 2. 背景

为了解用户使用情况、追踪应用版本分布和下载渠道效果，需要在 WSL Bridge 中集成 Microsoft Clarity 埋点能力。

## 3. 埋点数据设计

### 3.1 用户唯一标识（UID）

| 字段 | 类型 | 说明 |
|------|------|------|
| `user_uid` | `string` | 用户首次打开应用时生成的 UUID，持久化存储 |

**生成规则**：
- 使用 Rust `uuid::Uuid::new_v4()` 生成标准 UUID v4
- 生成时机：`sqlite_store.rs` 的 `load_snapshot()` 中，若 `user_uid` 为空则生成并保存
- 存储在 SQLite `app_setting` 表的 `AppSettings` 结构中
- 前端无需判断，`get_app_settings` 直接返回有效 UID

**上报方式**：
- 调用 Clarity Identify API：`Clarity.identify(user_uid)`
- 用户跨设备旅程追踪

### 3.2 应用版本号

| 字段 | 类型 | 说明 |
|------|------|------|
| `version` | `string` | 应用版本号，取自 `package.json` 的 `version` 字段 |

**注入方式**：
- Vite 构建时通过 `define` 选项注入全局常量 `__APP_VERSION__`
- 前端代码直接访问，无需运行时读取文件

**上报方式**：
- 调用 Clarity Custom Tags API：`Clarity.setTag("version", __APP_VERSION__)`

### 3.3 下载渠道

| 字段 | 类型 | 说明 |
|------|------|------|
| `channel` | `string` | 应用下载渠道，取自构建时环境变量 `VITE_APP_CHANNEL` |

**缺省值**：`"default"`

**注入方式**：
- 使用 Vite `VITE_` 前缀环境变量，自动注入到 `import.meta.env`
- 构建命令：`$env:VITE_APP_CHANNEL="github"; pnpm build`（Windows PowerShell）
- 未设置时前端代码使用 `"default"`

**上报方式**：
- 调用 Clarity Custom Tags API：`Clarity.setTag("channel", import.meta.env.VITE_APP_CHANNEL || "default")`

## 4. 技术实现方案

### 4.1 UID 持久化方案选择

| 方案 | Store Plugin | SQLite（选用） |
|------|-------------|---------------|
| 改动范围 | 前端 + Rust 安装插件 | Rust（AppSettings + sqlite_store） |
| 依赖 | 新增 `@tauri-apps/plugin-store` | 无新依赖 |
| 数据位置 | `analytics.json`（独立） | `state.db`（集中） |
| 调用方式 | JS 直接读写 | Tauri Command |
| 架构一致性 | 两套持久化系统 | 复用现有架构 |

**结论**：采用 SQLite 方案，理由：
1. `user_uid` 与 `AppSettings` 同属"用户配置"，应集中存储
2. 零新依赖，改动仅限 Rust 层
3. 前端通过 `get_app_settings` 一次性获取全部配置

### 4.2 实现步骤

#### 步骤 1：Rust 层扩展

**修改 `AppSettings` 结构体**（`src-tauri/crates/shared/src/lib.rs`）：
```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct AppSettings {
    pub close_behavior: CloseBehavior,
    pub show_tray_on_start: bool,
    pub user_uid: Option<String>,  // 新增字段
}
```

**修改 `sqlite_store.rs` 的 `load_snapshot()`**：
```rust
use uuid::Uuid;

// 在 load_snapshot() 中，app_settings 加载完成后
if app_settings.user_uid.is_none() {
    app_settings.user_uid = Some(Uuid::new_v4().to_string());
}
```

> 项目已依赖 `uuid` crate（见 `engine.rs`），无需新增依赖。

#### 步骤 2：Vite 配置修改

**修改 `vite.config.ts`**：
```typescript
import { defineConfig } from "vite";
import solid from "vite-plugin-solid";
import { readFileSync } from "fs";

const pkg = JSON.parse(readFileSync("./package.json", "utf-8"));

export default defineConfig({
  plugins: [solid()],
  define: {
    __APP_VERSION__: JSON.stringify(pkg.version),
  },
  // ...其他配置
});
```

> `__APP_VERSION__` 必须通过 `define` 注入（来自 `package.json`）。
> `VITE_APP_CHANNEL` 使用 Vite 环境变量机制，无需手动 `define`。

#### 步骤 3：TypeScript 类型定义

**创建 `src/vite-env.d.ts`**：
```typescript
/// <reference types="vite/client" />

declare const __APP_VERSION__: string;

interface ImportMetaEnv {
  readonly VITE_APP_CHANNEL: string;
  readonly VITE_CLARITY_PROJECT_ID: string;
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}
```

#### 步骤 4：前端 Clarity 增强

**修改 `src/lib/clarity.ts`**：
```typescript
import Clarity from "@microsoft/clarity";
import { invokeBridge } from "./bridge";

declare const __APP_VERSION__: string;

export function initClarity(projectId: string) {
  Clarity.init(projectId);
}

export async function setupClarityTracking(projectId: string) {
  Clarity.init(projectId);

  try {
    const settings = await invokeBridge<{
      close_behavior: string;
      show_tray_on_start: boolean;
      user_uid: string;
    }>("get_app_settings");

    Clarity.identify(settings.user_uid);

    Clarity.setTag("version", __APP_VERSION__);
    Clarity.setTag("channel", import.meta.env.VITE_APP_CHANNEL || "default");
  } catch (err) {
    console.warn("Failed to setup Clarity tracking:", err);
  }
}
```

> 前端无需判断 UID 是否存在，Rust 层 `load_snapshot()` 已保证 `user_uid` 有效。
> 使用 `Clarity.setTag()` API 替代 `window.clarity("set", ...)`。

**修改 `src/main.tsx`**：
```typescript
// 替换原有 initClarity 调用
if (!import.meta.env.DEV && import.meta.env.VITE_CLARITY_PROJECT_ID) {
  setupClarityTracking(import.meta.env.VITE_CLARITY_PROJECT_ID);
}
```

## 5. Clarity Dashboard 查看

埋点数据上报后，可在 Clarity Dashboard 的 **Filters** 中筛选查看：

- **Custom User ID**：按 `user_uid` 筛选用户
- **Custom Tags**：
  - `version`：按应用版本筛选
  - `channel`：按下载渠道筛选

## 6. 构建示例

```bash
# 默认渠道构建
pnpm build

# GitHub Release 构建
$env:VITE_APP_CHANNEL="github"; pnpm build

# Microsoft Store 构建
$env:VITE_APP_CHANNEL="msstore"; pnpm build
```

## 7. 注意事项

1. **开发模式不启用**：`!import.meta.env.DEV` 条件保护，避免开发调试数据污染
2. **UID Rust 生成**：首次数据库加载时由 Rust 自动生成，前端无需判断或生成
3. **渠道构建时注入**：非运行时读取，需在 CI/CD 或构建命令中设置环境变量
4. **API 调用顺序**：必须先 `Clarity.init()`，再调用 `Clarity.identify()` 和 `Clarity.setTag()`
5. **使用 TypeScript API**：`@microsoft/clarity` 包提供 `Clarity.setTag()` 方法，而非 `window.clarity("set", ...)`

## 8. 参考文档

- [Clarity Client API](https://learn.microsoft.com/en-us/clarity/setup-and-installation/clarity-api)
- [Clarity Identify API](https://learn.microsoft.com/en-us/clarity/setup-and-installation/identify-api)