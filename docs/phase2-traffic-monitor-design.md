# 二期流量监控与日志体系设计

## 1. 文档信息

- 项目名称：`wsl-bridge`
- 文档版本：`v1.4`
- 更新时间：`2026-04-16`
- 目标读者：产品、研发、测试

## 2. 范围与目标

### 2.1 一期回顾

一期（M1-M3）已完成：
- Tauri 2 应用骨架、规则 CRUD、TCP/UDP 转发
- HTTP/SOCKS5 代理执行器
- WSL/Hyper-V 拓扑探测、运行态管理
- 基础 UI/UX（Dashboard、Rules、Runtime、Topology、Settings）

### 2.2 二期目标

1. **流量监控**：实时统计规则转发流量，Dashboard 曲线图展示
2. **请求日志**：设计 access log 和 error log 的 JSON Lines 日志体系
3. **系统托盘**：支持最小化到系统托盘，后台继续运行
4. **关闭拦截**：拦截窗口关闭，提供关闭/最小化到托盘选项
5. **MCP 扩展**：暴露流量统计查询能力

### 2.3 本期不包含

- 日志查看界面（后续版本）
- 流量历史回放（后续版本）
- Error log 查询接口（后续版本）

## 3. 流量监控方案

### 3.1 架构设计

采用 **执行器秒级聚合 + Channel 推送 + 内存窗口 + SQLite 持久化** 三层架构：

```
┌─────────────────┐
│  Forwarder      │ ── 1s Aggregate ──→ Tauri Event Channel
│  (TCP/UDP/HTTP/ │                      (实时推送)
│   SOCKS5)       │
└─────────────────┘
                          │
                          ▼
┌─────────────────────────────────────────────┐
│  TrafficAggregator (Rust)                   │
│  ┌─────────────────────────────────────────┐│
│  │ Memory Window (每规则 60 秒滑动窗口)    ││
│  │ [{time, bytes_in, bytes_out, conn_cnt}]││
│  └─────────────────────────────────────────┘│
│  ┌─────────────────────────────────────────┐│
│  │ Minute Bucket (分钟聚合，定期写入 DB)   ││
│  └─────────────────────────────────────────┘│
└─────────────────────────────────────────────┘
                          │
                          ▼
┌─────────────────────────────────────────────┐
│  SQLite (traffic_stats)                     │
│  分钟级聚合数据，用于后续回放              │
└─────────────────────────────────────────────┘
```

### 3.2 数据模型

#### 3.2.1 TrafficEvent（秒级聚合推送）

```rust
pub struct TrafficEvent {
    pub rule_id: String,
    pub timestamp: i64,       // Unix 秒桶起点（ms）
    pub bytes_in: u64,
    pub bytes_out: u64,
    pub duration_ms: u64,
    pub connections: u64,
    pub requests: u64,
}
```

说明：
- `TrafficEvent` 不是原始 I/O 事件。
- 各执行器内部先按 1 秒时间桶聚合，再向 UI 和 `TrafficAggregator` 推送。
- 不直接推送每次 read/write 或每个请求结束事件，避免长连接统计失真和高频 Channel 压力。

#### 3.2.2 Memory Window（内存滑动窗口）

每条规则维护一个环形缓冲区：

```rust
pub struct TrafficWindow {
    pub rule_id: String,
    pub samples: Vec<TrafficSample>,  // 固定容量（窗口秒数）
    pub head: usize,                  // 写入位置
}

pub struct TrafficSample {
    pub timestamp: i64,       // Unix 秒级（对齐）
    pub bytes_in: u64,
    pub bytes_out: u64,
    pub connections: u64,
    pub total_duration_ms: u64,
}
```

#### 3.2.3 SQLite traffic_stats 表

```sql
CREATE TABLE traffic_stats (
    id TEXT PRIMARY KEY,              -- UUID v4
    rule_id TEXT NOT NULL,
    time_bucket INTEGER NOT NULL,     -- Unix timestamp，按分钟对齐
    bytes_in INTEGER NOT NULL DEFAULT 0,
    bytes_out INTEGER NOT NULL DEFAULT 0,
    connections INTEGER NOT NULL DEFAULT 0,
    requests INTEGER NOT NULL DEFAULT 0,
    total_duration_ms INTEGER NOT NULL DEFAULT 0,
    avg_duration_ms INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL       -- 记录创建时间
);

CREATE UNIQUE INDEX idx_traffic_stats_rule_bucket 
    ON traffic_stats(rule_id, time_bucket);
CREATE INDEX idx_traffic_stats_time ON traffic_stats(time_bucket);
CREATE INDEX idx_traffic_stats_id ON traffic_stats(id);
```

**ID 设计说明**：

使用 UUID v4 作为主键：
- 避免 INTEGER 自增 ID 的潜在问题（合并数据冲突、暴露业务量）
- UUID 字符串格式：`xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx`
- 使用 `uuid` crate 生成：`uuid::Uuid::new_v4().to_string()`

### 3.3 存储管理策略

| 精度 | 保留时长 | 用途 |
|------|----------|------|
| 秒级（内存） | 窗口大小（30s/60s/120s） | Dashboard 实时曲线 |
| 分钟级（DB） | 7 天 | 近期数据，供后续回放 |

#### 定期任务

1. **分钟写入**：定时将分钟聚合数据写入 `traffic_stats`
2. **过期清理**：每日清理超过保留期限的数据

### 3.4 前端 Dashboard 曲线图

#### 3.4.1 图表库

使用 **uPlot**（https://github.com/leeoniya/uPlot）：
- 文件小（~25KB gzipped）
- 时序数据渲染性能极高
- 支持 Canvas 渲染，适合高频刷新

#### 3.4.2 配置项

| 配置 | 选项 | 默认值 | 说明 |
|------|------|--------|------|
| 窗口大小 | 30s / 60s / 120s | 60s | 曲线显示的时间范围 |
| 刷新率 | 1s / 2s / 5s | 1s | 曲线更新频率 |
| 显示指标 | 总流量 / 入流量 / 出流量 / 连接数 | 总流量 | 曲线 Y 轴指标 |
| 规则选择 | 全部 / 指定规则 | 全部 | 显示哪些规则的曲线 |
| 规则颜色 | 自动分配 / 手动配置 | 自动分配 | 每条规则的曲线颜色 |

#### 3.4.3 默认色板

```
#36A2EB  蓝色
#FF6384  红色
#FFCE56  黄色
#4BC0C0  青色
#9966FF  紫色
#FF9F40  橙色
#4DC9F6  浅蓝
#C9CBCF  灰色
#7C4DFF  深紫
#00D9FF  天蓝
```

按规则创建顺序循环分配，用户可在配置 Modal 中自定义。

#### 3.4.4 配置 Modal

位于图表右上角，点击按钮弹出：

- 窗口大小选择（下拉）
- 刷新率选择（下拉）
- 显示指标选择（下拉）
- 规则勾选（多选框，默认全选）
- 规则颜色配置（颜色选择器，每规则一行）

#### 3.4.5 曲线交互

本期只做实时刷新 + 固定窗口：
- 曲线从右向左滚动（时间轴）
- 最新数据在最右侧
- 无缩放、拖拽、回放功能

#### 3.4.6 Tab 可见性优化

当 Dashboard Tab Panel 不处于活动状态时，需要优化性能：

**问题分析**：
- 流量事件持续通过 Channel 推送
- uPlot 图表持续刷新消耗 CPU
- 内存窗口数据持续累积
- 前端订阅 Channel 消耗资源

**优化策略**：

| 状态 | 行为 | 资源消耗 |
|------|------|----------|
| Dashboard 活动 | Channel 监听 + 图表刷新 | 正常 |
| Dashboard 非活动 | Channel 监听停止 + 图表销毁 | 最小 |

**实现方案**：

```typescript
// 前端：监听 Tab 切换事件
useEffect(() => {
  const unsubscribe = listenTrafficEvent((event) => {
    // 更新图表数据
  });
  
  // Tab 切换时清理
  onCleanup(() => {
    unsubscribe();
    destroyChart();
  });
});
```

**关键点**：

1. **Channel 监听**：Dashboard 非活动时，前端停止监听 `traffic:event`
2. **图表销毁**：uPlot 实例销毁，释放 Canvas 内存
3. **数据保留**：内存窗口数据保留在 Rust 层，切换回来时重新拉取
4. **无缝恢复**：切换回 Dashboard 时：
   - 重新订阅 Channel
   - 重新创建 uPlot 实例
   - 从 Rust 层拉取当前窗口数据填充图表

**API 设计**：

```rust
pub struct TrafficWindowData {
    pub rule_id: String,
    pub samples: Vec<TrafficSample>,
}

// 新增 command：获取当前窗口数据
#[tauri::command]
fn get_traffic_window_data(rule_ids: Vec<String>) -> Vec<TrafficWindowData>;
```

## 4. 日志体系方案

### 4.1 设计参考

采用 JSON Lines 日志体系：
- **access.log**：记录每个请求的基本信息
- **error.log**：记录错误和异常信息

### 4.2 日志文件路径

```
%ProgramData%\wsl-bridge\logs\
  ├── access.log           -- 当前 access log
  ├── access.log.1         -- 前一天的 access log（轮转）
  ├── access.log.2         -- 更早的 access log
  ├── error.log            -- 当前 error log
  ├── error.log.1          -- 前一天的 error log（轮转）
  └── error.log.2          -- 更早的 error log
```

### 4.3 Access Log 格式

采用 JSON Lines，每行一个 JSON 对象：

```json
{"ts":"2026-04-15T10:23:45.123Z","rule_id":"rule-001","client":"192.168.1.100:54321","protocol":"tcp","method":"CONNECT","target":"172.20.10.2:8080","status":"success","bytes_in":1024,"bytes_out":2048,"duration_ms":156}
{"ts":"2026-04-15T10:23:46.456Z","rule_id":"rule-002","client":"192.168.1.101:54322","protocol":"http","method":"GET","target":"172.20.10.3:3000/api/users","status":"success","bytes_in":256,"bytes_out":1024,"duration_ms":89}
```

#### 字段说明

| 字段 | 说明 |
|------|------|
| ts | ISO 8601 时间戳（带毫秒） |
| rule_id | 关联规则 ID |
| client | 客户端地址 |
| protocol | tcp / udp / http / socks5 |
| method | CONNECT / GET / POST / PUT 等 |
| target | 目标地址（解析后） |
| status | success / error / timeout / refused |
| bytes_in | 入流量（字节） |
| bytes_out | 出流量（字节） |
| duration_ms | 持续时间（毫秒） |

### 4.4 Error Log 格式

```json
{"ts":"2026-04-15T10:23:47.789Z","rule_id":"rule-003","error_type":"connect_timeout","error_message":"Connection timeout after 30s","client":"192.168.1.102","target":"172.20.10.4:22","detail":{"retry_count":3}}
{"ts":"2026-04-15T10:24:00.123Z","rule_id":"rule-001","error_type":"target_refused","error_message":"Target refused connection","client":"192.168.1.100","target":"172.20.10.2:8080","detail":null}
```

#### 字段说明

| 字段 | 说明 |
|------|------|
| ts | ISO 8601 时间戳 |
| rule_id | 关联规则 ID（可为空，如拓扑错误） |
| error_type | connect_timeout / target_refused / protocol_error / topology_error 等 |
| error_message | 错误描述 |
| client | 客户端 IP（可选） |
| target | 目标 IP（可选） |
| detail | 额外信息（可选，JSON 对象或 `null`） |

### 4.5 日志轮转技术选型

#### 4.5.1 可选方案对比

| 方案 | 优点 | 缺点 | 适用度 |
|------|------|------|--------|
| **tracing-appender** | tracing 官方出品、稳定度高、非阻塞写入、已集成 tracing | 仅支持时间轮转（不支持大小轮转） | ⭐⭐⭐ |
| **log4rs** | 功能丰富、支持大小+时间复合轮转 | 依赖较重、使用 log facade 需桥接 tracing | ⭐⭐ |
| **手写实现** | 完全自定义、支持大小+时间双重轮转、无额外依赖 | 需处理并发安全、缓冲刷新、错误处理 | ⭐⭐⭐⭐ |

#### 4.5.2 业务需求分析

本项目日志特点：
- 需要独立的 `access.log` 和 `error.log` 两个文件
- 需要自定义格式（JSON Lines，非标准 tracing 格式）
- 需要大小 + 时间双重轮转策略
- 高频写入场景（每个转发请求一次 access log）

#### 4.5.3 推荐方案：手写 RollingFileAppender

基于 `tracing-appender::non_blocking` 架构，自定义 RollingFileWriter：

```rust
pub struct RollingFileWriter {
    inner: BufWriter<File>,
    current_path: PathBuf,
    base_path: PathBuf,
    rotation_size: u64,         // 10MB
    max_rotations: usize,       // 7
    current_size: u64,
    last_rotation: DateTime<Utc>,
}

impl RollingFileWriter {
    pub fn write_entry(&mut self, entry: &str) -> io::Result<()> {
        self.inner.write_all(entry.as_bytes())?;
        self.current_size += entry.len() as u64;
        
        // 检查是否需要轮转
        if self.current_size >= self.rotation_size || 
           self.should_time_rotate() {
            self.rotate()?;
        }
        Ok(())
    }
    
    fn rotate(&mut self) -> io::Result<()> {
        self.inner.flush()?;
        // 重命名当前文件为 .1
        // 创建新文件
        // 清理超过 max_rotations 的文件
    }
}
```

使用 `tracing_appender::non_blocking` 包装实现非阻塞写入：

```rust
let writer = RollingFileWriter::new(log_dir, "access.log", config);
let (non_blocking, guard) = tracing_appender::non_blocking(writer);
```

#### 4.5.4 关键依赖

```toml
[dependencies]
tracing-appender = "0.2"    # 非阻塞写入框架
uuid = { version = "1.0", features = ["v4"] }  # UUID 生成
```

### 4.6 日志轮转策略

| 参数 | 默认值 | 说明 |
|------|--------|------|
| rotation_size | 10MB | 单文件超过此大小时轮转 |
| rotation_time | daily | 每天轮转（即使未达大小限制） |
| max_rotations | 7 | 保留轮转文件数量 |
| retention_days | 7 | 超过此天数的日志删除 |

### 4.7 现有 Logs 页面处理

删除现有 `/logs` 页面：
- 移除路由配置
- 移除页面组件代码
- 移除导航 Tab

现有 `query_logs` command 暂不作为 UI 能力使用；如后端内部仍有依赖可暂时保留实现。

## 5. 系统托盘与关闭拦截

### 5.1 功能需求

1. **关闭拦截**：点击窗口关闭按钮时，不直接关闭，而是询问用户操作
2. **最小化到托盘**：用户选择后，隐藏窗口并显示系统托盘图标
3. **后台运行**：窗口隐藏后，应用继续运行，流量统计和日志记录正常工作
4. **托盘菜单**：提供快捷操作菜单（显示窗口、退出应用）

### 5.2 交互流程

```
用户点击关闭按钮
        │
        ▼
┌───────────────────────┐
│   关闭确认对话框       │
│                       │
│  [最小化到托盘]       │  ← 默认选项
│  [退出应用]           │
│  [取消]               │
└───────────────────────┘
        │
   ┌────┴────┐
   │         │
   ▼         ▼
最小化     退出应用
到托盘     应用
   │
   ▼
隐藏窗口
显示托盘图标
继续后台运行
```

### 5.3 技术实现

#### 5.3.1 关闭拦截（前端）

```typescript
import { getCurrentWindow } from '@tauri-apps/api/window';

const appWindow = getCurrentWindow();

appWindow.onCloseRequested(async (event) => {
  // 阻止默认关闭行为
  event.preventDefault();
  
  // 显示确认对话框
  const choice = await showCloseConfirmDialog();
  
  if (choice === 'minimize') {
    await appWindow.hide();
    // 托盘图标已在启动时创建
  } else if (choice === 'quit') {
    // 显式退出应用
    await appWindow.close();
  }
  // choice === 'cancel' 时什么都不做
});
```

#### 5.3.2 系统托盘（Rust）

```rust
use tauri::{
    menu::{MenuBuilder, MenuItemBuilder},
    tray::{TrayIconBuilder, TrayIconEvent, MouseButton, MouseButtonState},
    Manager,
};

fn setup_tray(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    // 创建托盘菜单
    let show_item = MenuItemBuilder::with_id("show", "显示窗口").build(app)?;
    let quit_item = MenuItemBuilder::with_id("quit", "退出应用").build(app)?;
    let menu = MenuBuilder::new(app)
        .items(&[&show_item, &quit_item])
        .build()?;
    
    // 创建托盘图标
    let tray = TrayIconBuilder::new()
        .icon(app.default_window_icon().unwrap().clone())
        .menu(&menu)
        .menu_on_left_click(true)
        .tooltip("WSL Bridge")
        .on_menu_event(|app, event| match event.id().as_ref() {
            "show" => {
                if let Some(window) = app.get_webview_window("main") {
                    window.show().ok();
                    window.unminimize().ok();
                    window.set_focus().ok();
                }
            }
            "quit" => {
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            // 单击托盘图标显示窗口
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event {
                let app = tray.app_handle();
                if let Some(window) = app.get_webview_window("main") {
                    window.show().ok();
                    window.unminimize().ok();
                    window.set_focus().ok();
                }
            }
        })
        .build(app)?;
    
    Ok(())
}
```

#### 5.3.3 托盘图标资源

托盘图标建议使用 16x16 或 32x32 的 ICO/PNG 格式：

```
src-tauri/app/icons/
  ├── icon.ico           -- 应用图标（现有）
  └── tray-icon.ico      -- 托盘图标（新增，小尺寸）
```

**图标暂缺说明**：

当前缺少专用托盘图标资源，临时方案：
- 正常状态：使用应用图标代替
- 灰色状态：使用应用图标代替
- 错误状态（红色角标）：使用应用图标代替

在代码中替换图标位置添加标记：`// TODO: need icon`

### 5.4 配置项

用户可在 Settings 页面配置关闭行为：

| 配置 | 选项 | 默认值 |
|------|------|--------|
| 关闭时默认操作 | 询问 / 直接最小化 / 退出应用 | 询问 |
| 启动时显示托盘 | 是 / 否 | 是 |

### 5.6 Settings 数据表

新增 `settings` 表存储用户配置：

```sql
CREATE TABLE settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at INTEGER NOT NULL
);

-- 默认配置
-- close_behavior: ask / minimize / close
-- show_tray_on_start: true / false
INSERT INTO settings VALUES 
    ('close_behavior', 'ask', 0),
    ('show_tray_on_start', 'true', 0);
```

### 5.7 状态同步

托盘图标需要反映应用状态：

| 状态 | 托盘图标表现 |
|------|--------------|
| 有规则运行 | 正常图标 |
| 无规则运行 | 灰色图标或添加角标 |
| 有错误 | 添加红色角标 |

**实现方案**：通过 Tauri Event 同步状态：

```rust
// Rust 层：状态变化时更新托盘
app_handle.emit("runtime_status_changed", status);

// 托盘状态更新逻辑
fn update_tray_icon(tray: &TrayIcon, status: &RuntimeStatus) {
    match status {
        RuntimeStatus::Running => tray.set_icon(normal_icon),   // TODO: need icon
        RuntimeStatus::Stopped => tray.set_icon(gray_icon),     // TODO: need icon
        RuntimeStatus::Error => tray.set_icon(error_icon),      // TODO: need icon
    }
}
```

## 6. MCP 扩展

### 6.1 新增工具

| 工具名称 | 说明 | 参数 |
|----------|------|------|
| `query_traffic_stats` | 查询单规则流量统计 | `rule_id`, `start_time`, `end_time`, `interval` |
| `get_traffic_window` | 获取实时窗口数据 | `rule_id`, `window_size` |

### 6.2 query_traffic_stats 返回

```json
{
  "stats": [
    {
      "time_bucket": 1713120000,
      "rule_id": "rule-001",
      "bytes_in": 102400,
      "bytes_out": 204800,
      "connections": 15
    }
  ],
  "total_bytes_in": 512000,
  "total_bytes_out": 1024000,
  "total_connections": 75
}
```

### 6.3 配置项扩展

`mcp_config` 新增字段：

```json
{
  "expose_traffic_stats": true
}
```

## 7. 界面调整

### 7.1 Dashboard 改造

现有 Dashboard 增加流量监控区块：

```
┌────────────────────────────────────────────────────┐
│  Dashboard                                         │
├────────────────────────────────────────────────────┤
│  ┌──────────┐  ┌──────────┐  ┌──────────┐         │
│  │ 应用状态 │  │ 规则状态 │  │ 风险提示 │         │
│  └──────────┘  └──────────┘  └──────────┘         │
├────────────────────────────────────────────────────┤
│  ┌──────────────────────────────────────────────┐ │
│  │ 流量监控                              [设置] │ │
│  │                                              │ │
│  │     [uPlot 曲线图区域]                      │ │
│  │                                              │ │
│  │  时间轴 ←──────────────────────────────→    │ │
│  └──────────────────────────────────────────────┘ │
└────────────────────────────────────────────────────┘
```

### 7.2 导航调整

移除 Logs Tab，导航变为：
- Dashboard
- Rules
- Runtime
- Topology
- Settings

## 8. 技术栈补充

### 8.1 新增依赖

| 依赖 | 用途 | 版本 |
|------|------|------|
| uPlot (JS) | 流量曲线图 | latest |
| tracing-appender (Rust) | 非阻塞日志写入框架 | 0.2 |
| uuid (Rust) | UUID 生成 | 1.0 |

### 8.2 Cargo.toml 新增配置

```toml
[dependencies]
tracing-appender = "0.2"
uuid = { version = "1.0", features = ["v4"] }
```

### 8.3 现有依赖调整

- 移除 Logs 页面相关前端代码
- `query_logs` 不再作为本期 UI 能力使用

## 9. 里程碑计划

### 9.1 工作量与难度分析

| 功能模块 | 工作量 | 难度 | 技术风险 | 依赖 |
|----------|--------|------|----------|------|
| **流量监控核心** | | | | |
| TrafficAggregator（内存窗口） | 2 天 | 中 | 中 | 无 |
| TrafficEvent Channel 推送 | 0.5 天 | 低 | 低 | TrafficAggregator |
| SQLite traffic_stats 表 | 0.5 天 | 低 | 低 | 无 |
| 流量写入持久化 | 1 天 | 中 | 低 | SQLite 表 |
| **流量图表 UI** | | | | |
| uPlot 基础集成 | 1.5 天 | 中 | 中 | 无 |
| 实时数据绑定 | 1 天 | 中 | 中 | Channel 推送 + uPlot |
| 配置 Modal | 1 天 | 低 | 低 | uPlot |
| Tab 可见性优化 | 1 天 | 中 | 中 | uPlot + Channel |
| **日志体系** | | | | |
| RollingFileWriter | 2 天 | 中 | 中 | 无 |
| Access log 写入 | 0.5 天 | 低 | 低 | RollingFileWriter |
| Error log 写入 | 0.5 天 | 低 | 低 | RollingFileWriter |
| 日志轮转清理 | 1 天 | 低 | 低 | RollingFileWriter |
| **系统托盘** | | | | |
| TrayIcon 集成 | 0.5 天 | 低 | 低 | 无 |
| 关闭拦截 Dialog | 0.5 天 | 低 | 低 | TrayIcon |
| 托盘菜单 | 0.5 天 | 低 | 低 | TrayIcon |
| 状态同步 | 0.5 天 | 低 | 低 | TrayIcon |
| **MCP 扩展** | | | | |
| query_traffic_stats | 0.5 天 | 低 | 低 | SQLite traffic_stats |
| get_traffic_window | 0.5 天 | 低 | 低 | 内存窗口 |
| **界面调整** | | | | |
| Dashboard 改造 | 1 天 | 低 | 低 | 流量图表 |
| 删除 Logs 页面 | 0.5 天 | 低 | 低 | 无 |

**总工作量估算**：约 14 天（单人全职开发）

### 9.2 里程碑划分

#### M5.1 流量监控核心（预计 4 天）

**目标**：完成流量统计后端基础设施，提供数据查询能力

| 任务 | 工时 | 交付物 |
|------|------|--------|
| SQLite traffic_stats 表 + UUID 主键 | 0.5 天 | 数据库表、索引 |
| TrafficAggregator 内存窗口设计 | 2 天 | Rust TrafficAggregator 模块 |
| TrafficEvent Channel 推送集成 | 0.5 天 | Forwarder 集成事件推送 |
| 流量写入持久化（分钟聚合） | 1 天 | 定时写入任务、批量写入 |

**验收标准**：
1. `traffic_stats` 表正确创建，ID 使用 UUID
2. 转发请求按 1 秒聚合触发 TrafficEvent 推送
3. 内存窗口正确维护每规则滑动数据
4. 分钟数据正确写入 SQLite

**技术风险点**：
- 高频聚合事件推送可能堵塞 Channel → 使用非阻塞 emit
- SQLite 写入频繁 → 每 10 秒批量写入一次

#### M5.2 流量图表 UI（预计 4 天）

**目标**：Dashboard 集成实时流量曲线图，用户可配置显示参数

| 任务 | 工时 | 交付物 |
|------|------|--------|
| uPlot 依赖引入 + 基础封装 | 1.5 天 | 前端 TrafficChart 组件 |
| Channel 监听 + 实时数据绑定 | 1 天 | 前端实时刷新逻辑 |
| 配置 Modal（窗口/刷新率/指标/颜色） | 1 天 | 前端 TrafficConfigModal |
| Tab 可见性优化（销毁/恢复） | 0.5 天 | 前端生命周期管理 |

**验收标准**：
1. Dashboard 显示流量曲线图，1 秒刷新无卡顿
2. 曲线显示最近 60 秒数据，从右向左滚动
3. 配置 Modal 可调整窗口大小（30s/60s/120s）、刷新率（1s/2s/5s）
4. 切换到其他 Tab 时图表销毁，切换回来时无缝恢复
5. 多规则同时显示，不同颜色区分

**技术风险点**：
- uPlot API 学习曲线 → 预研阶段提前阅读文档
- Tab 切换内存泄漏 → useEffect onCleanup 严格清理

**依赖**：M5.1（需要 Channel 推送和内存窗口数据）

#### M5.3 日志体系（预计 4 天）

**目标**：建立完整的 access log 和 error log 文件日志体系

| 任务 | 工时 | 交付物 |
|------|------|--------|
| RollingFileWriter（大小+时间轮转） | 2 天 | Rust RollingFileWriter 模块 |
| Access log 格式 + 写入 | 0.5 天 | Forwarder 集成 access JSON Lines log |
| Error log 格式 + 写入 | 0.5 天 | Error 事件集成 error JSON Lines log |
| 日志轮转 + 清理 | 1 天 | 定时轮转任务、过期删除 |

**验收标准**：
1. `access.log` 正确记录每个请求，格式符合 JSON Lines 规范
2. `error.log` 正确记录错误事件
3. 单文件超过 10MB 或每天触发轮转
4. 轮转文件正确重命名（.1/.2/.3...）
5. 超过 7 天的日志自动删除

**技术风险点**：
- 高频写入影响性能 → tracing_appender::non_blocking 包装
- 文件句柄管理 → 确保轮转时正确关闭旧文件

**依赖**：无（可与 M5.2 并行开发）

#### M5.4 系统托盘 + MCP + 界面收尾（预计 2 天）

**目标**：完成系统托盘功能、MCP 扩展、界面最终调整

| 任务 | 工时 | 交付物 |
|------|------|--------|
| TrayIcon 集成 + 托盘图标资源 | 0.5 天 | Rust TrayIcon、tray-icon.ico |
| 关闭拦截 + 确认对话框 | 0.5 天 | 前端 CloseConfirmModal |
| 托盘菜单 + 状态同步 | 0.5 天 | 托盘菜单、状态图标 |
| MCP query_traffic_stats | 0.5 天 | MCP 工具实现 |

**验收标准**：
1. 点击关闭按钮显示确认对话框（最小化到托盘/退出应用/取消）
2. 选择最小化到托盘后窗口隐藏、托盘图标显示
3. 单击托盘图标恢复窗口
4. 托盘菜单提供"显示窗口"、"退出应用"选项
5. MCP `query_traffic_stats` 可按单 `rule_id` 查询流量统计
6. Logs 页面已删除

**依赖**：M5.1（MCP 需要流量数据）

### 9.3 开发顺序建议

```
时间线（14 天）

Week 1:
  Day 1-2:  M5.1 SQLite + TrafficAggregator（可独立进行）
  Day 3-4:  M5.1 Channel + 持久化
  Day 3-4:  M5.3 RollingFileWriter（可与 M5.1 后半并行）

Week 2:
  Day 5-6:  M5.2 uPlot 基础 + 实时绑定
  Day 7:    M5.2 配置 Modal
  Day 8:    M5.2 Tab 可见性 + M5.3 日志轮转收尾
  Day 9-10: M5.4 系统托盘 + MCP + 界面收尾
```

**并行机会**：
- M5.1 后半（Channel/持久化）与 M5.3（RollingFileWriter）可并行
- M5.3 与 M5.2 可部分并行（日志写入与图表 UI 无依赖）

### 9.4 后续版本（M6）

- 日志查看界面（Web 界面查看 access/error log）
- 流量历史回放（选择历史时段查看）
- 流量报表导出（CSV/PDF 导出）
- 托盘图标状态增强（运行态角标）

## 10. 验收标准（按里程碑）

### 10.1 验收量化标准

| 指标 | 阈值 | 工具 |
|------|------|------|
| FPS | > 55 fps | Chrome DevTools |
自动化测试覆盖所有验收标准，测试必须 100% 通过。

### M5.1 验收标准

1. `traffic_stats` 表正确创建，ID 使用 UUID v4
2. 转发请求按 1 秒聚合触发 TrafficEvent 推送
3. 内存窗口正确维护每规则滑动数据（秒级精度）
4. 分钟聚合数据正确写入 SQLite
5. 后端提供 `get_traffic_window_data` command

### M5.2 验收标准

1. Dashboard 显示流量曲线图，曲线从右向左滚动
2. 实时数据 1 秒刷新，无卡顿
3. 配置 Modal 可调整：窗口大小（30s/60s/120s）、刷新率（1s/2s/5s）、显示指标、规则颜色
4. 切换到其他 Tab 时图表销毁、Channel 监听停止
5. 切换回 Dashboard 时图表无缝恢复
6. 多规则同时显示，颜色区分清晰

### M5.3 验收标准

1. `access.log` 正确记录每个请求，格式为 JSON Lines
2. `error.log` 正确记录错误事件，格式为 JSON Lines
3. 单文件超过 10MB 触发轮转
4. 每日触发时间轮转（即使未达大小限制）
5. 轮转文件正确重命名（access.log → access.log.1）
6. 超过 7 天的日志自动删除

### M5.4 验收标准

1. 点击窗口关闭按钮显示确认对话框
2. 对话框提供三个选项：最小化到托盘、退出应用、取消
3. 选择最小化后窗口隐藏、托盘图标显示
4. 单击托盘图标恢复窗口并聚焦
5. 托盘右键菜单：显示窗口、退出应用
6. 窗口隐藏后流量统计和日志记录继续工作
7. MCP `query_traffic_stats` 可按单 `rule_id` 查询指定时间范围的流量统计
8. Logs 页面已删除，导航显示 5 个 Tab

## 11. 风险与应对

| 风险 | 影响里程碑 | 应对策略 |
|------|------------|----------|
| 高频聚合事件导致 Channel 堵塞 | M5.1 | 使用非阻塞 emit，设置 backlog 丢弃策略 |
| SQLite 分钟写入频繁 | M5.1 | 每 10 秒批量写入一次分钟桶，减少 I/O |
| uPlot API 学习曲线 | M5.2 | 开发前预研，提前阅读官方文档和示例 |
| Tab 切换内存泄漏 | M5.2 | useEffect onCleanup 严格清理，添加单元测试 |
| uPlot 中文字体渲染 | M5.2 | 测试多种字体，必要时调整 CSS font-family |
| RollingFileWriter 轮转逻辑复杂 | M5.3 | 先实现基础版本，逐步添加清理 |
| 高频日志写入影响性能 | M5.3 | tracing_appender::non_blocking 包装 |
| 文件句柄未正确关闭 | M5.3 | 轮转时确保 flush + close，添加错误处理 |
| 托盘图标不同 Windows 版本显示差异 | M5.4 | 提供多尺寸 ICO 文件（16x16, 32x32, 48x48） |
| 用户习惯点击关闭按钮 | M5.4 | Settings 提供配置项，允许自定义关闭行为 |

## 12. 技术预研建议

在正式开发前，建议完成以下预研：

### M5.1 预研（1 天）

1. **TrafficAggregator 设计**：绘制详细架构图，确定数据结构
2. **Channel 性能测试**：编写简单 demo 测试秒级聚合事件推送性能
3. **SQLite 批量写入测试**：测试不同批量大小对写入性能的影响

### M5.2 预研（1 天）

1. **uPlot 基础用法**：阅读官方文档，实现简单 demo
2. **实时数据绑定**：测试 setInterval + uPlot.setData 性能
3. **组件销毁测试**：测试 Solid.js onCleanup + uPlot.destroy

### M5.3 预研（0.5 天）

1. **tracing_appender 用法**：阅读文档，测试 non_blocking 包装
2. **文件轮转逻辑**：编写简单 demo 测试大小判断 + 重命名

---
