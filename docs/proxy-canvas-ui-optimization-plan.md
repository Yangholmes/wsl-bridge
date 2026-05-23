# Proxy 画布化 UI 交互优化方案

## 1. 文档信息

- 项目名称：`wsl-bridge`
- 文档版本：`v1.0`
- 更新时间：`2026-05-18`
- 目标读者：产品、研发、测试
- 参考原型：`docs/screenshots/Proxy-UI.png`

## 2. 背景与目标

当前 `Proxy` 模块已经具备三期核心能力：

- `Listener -> Route -> Upstream` 分层配置
- HTTP / HTTPS Listener
- server name 与 path prefix 路由
- WSL / Hyper-V / Static 三类上游
- WebSocket、gRPC、grpcs 数据面
- 证书管理与本地 CA
- 运行态、错误态与迁移引导

但当前 UI 仍是三段列表式 CRUD：

- Listener / Route / Upstream 的层级关系不直观
- 操作入口分散，新增下游节点的指向性弱
- 多 Listener、多 Route、多 Upstream 情况下难以理解完整链路
- 错误态只能从列表文本中阅读，不能快速定位异常链路
- 运行态与配置关系没有形成可视化拓扑

本次优化目标是将 `Proxy` 的核心配置区改造成 PixiJS 拓扑画布，以更直接的方式表达：

```text
Listener -> Route -> Upstream[]
```

画布替代当前 `Listener / Route / Upstream` 三段列表区域；信息概览区、迁移提示、证书管理区域保留。

## 3. 范围边界

### 3.1 本次改造包含

- 使用 PixiJS 实现 Proxy 拓扑画布
- 画布完全替代当前 Listener / Route / Upstream 三段列表区域
- 保留顶部信息概览区
- 保留迁移提示，位置放在画布上方
- 保留证书管理区域，不纳入画布
- 保留现有新增/编辑 Modal 表单
- 增加画布下方选中节点详情面板
- 支持画布平移、缩放、回到内容区
- 支持空白处与节点的自定义上下文菜单
- 支持自动布局、布局过渡动画
- 支持搜索与定位
- 支持禁用态、错误态节点与链路高亮
- 支持级联删除二次确认

### 3.2 本次改造不包含

- 不把证书建模为画布节点
- 不在 Canvas 内直接编辑复杂表单字段
- 不支持节点手动拖拽改布局
- 不支持多选与批量操作
- 不保存画布视图状态到磁盘
- 不改变 Proxy 后端数据模型
- 不改变 Proxy 数据面执行逻辑

## 4. 页面信息架构

改造后的 `ProxyPage` 结构：

```text
Proxy Page
├─ PageHeader
├─ Metric Cards
├─ Migration Guide
├─ Proxy Canvas Section
│  ├─ Toolbar
│  │  ├─ 新建 Listener
│  │  ├─ 刷新
│  │  ├─ 搜索入口 / 搜索框
│  │  ├─ 回到内容区
│  │  └─ 缩放状态
│  ├─ PixiJS Canvas
│  ├─ Context Menu
│  └─ Selected Node Detail Panel
├─ Certificates Section
├─ Existing Modal Forms
└─ Delete Confirm Modal
```

其中：

- `Proxy Canvas Section` 替代原有 Listener / Route / Upstream 三段列表。
- `Certificates Section` 保持当前 UI 结构，最多做轻量样式对齐。
- `Modal Forms` 复用当前 Listener / Route / Upstream / Certificate 表单逻辑。

## 5. 画布节点模型

### 5.1 节点类型

画布节点分为三类：

```ts
type ProxyCanvasNodeKind = "listener" | "route" | "upstream";
```

### 5.2 节点字段

```ts
type ProxyCanvasNode = {
  id: string;
  kind: ProxyCanvasNodeKind;
  parentId: string | null;
  title: string;
  subtitle: string;
  enabled: boolean;
  hasError: boolean;
  runtimeError: string | null;
  source:
    | ProxyListener
    | ProxyRoute
    | ProxyUpstream;
};
```

字段含义：

- `id`：使用业务对象 id，前端再拼接 kind 形成唯一 key，例如 `listener:<id>`。
- `parentId`：Listener 为 `null`，Route 指向 Listener，Upstream 指向 Route。
- `title`：节点主文本。
- `subtitle`：节点规则摘要。
- `enabled`：禁用态灰色展示。
- `hasError`：运行态错误高亮。
- `runtimeError`：详情面板和 Tooltip 使用。
- `source`：保留原始业务对象，方便详情面板和编辑 Modal 复用。

### 5.3 节点文本规则

Listener 节点：

```text
title: listener.name
subtitle: `${protocol}://${listen_host}:${listen_port}`
```

Route 节点：

```text
title:
  route.is_default ? "/" : route.server_names.join(", ")

subtitle:
  route.path_prefix ? route.path_prefix : "default"
```

Upstream 节点：

```text
title: upstream.target_kind
subtitle:
  static: `${target_host}:${target_port}`
  wsl/hyperv: `${target_ref}:${target_port}`
```

## 6. 链路模型

### 6.1 链路类型

```ts
type ProxyCanvasEdge = {
  id: string;
  from: string;
  to: string;
  enabled: boolean;
  hasError: boolean;
};
```

### 6.2 错误高亮规则

- Listener runtime error：
  - Listener 节点红色高亮
- Route runtime error：
  - Route 节点红色高亮
  - `Listener -> Route` 边红色高亮
- Upstream runtime error：
  - Upstream 节点红色高亮
  - `Route -> Upstream` 边红色高亮

### 6.3 禁用态规则

- `enabled = false` 的节点使用灰色低对比样式
- 指向禁用节点的链路使用灰色虚线或低透明度线条
- 错误态优先级高于普通启用态
- 禁用态如果也有历史错误，节点仍以禁用态为主，详情面板展示错误信息

## 7. 自动布局方案

### 7.1 布局原则

采用固定层级、有序树布局：

```text
Layer 0: Listener
Layer 1: Route
Layer 2: Upstream
```

布局方向：

- 横向表达层级
- 纵向表达兄弟节点
- 多 Listener 纵向排列
- 每个 Listener 的 Route 和 Upstream 作为该 Listener 的子树展开

### 7.2 排序规则

排序沿用业务匹配优先级。

Listener 排序：

- 建议使用 `created_at` 升序，保证整体稳定
- 如果当前接口已有稳定顺序，优先沿用接口顺序

Route 排序：

1. 非默认路由优先于默认路由
2. server name 精确匹配优先于通配符
3. path prefix 更长的优先
4. 同匹配等级时按 `created_at` 倒序
5. 默认路由放在该 Listener 的 Route 列最后

Upstream 排序：

1. enabled 优先
2. 按业务选择优先级，当前三期口径为创建时间倒序
3. 同时间按 id 稳定排序

### 7.3 布局算法

核心目标：

- 新增兄弟节点后，只影响该子树及之后节点的纵向位置
- 节点之间距离稳定
- 不需要用户手动调整
- 数据变化后可以通过动画平滑过渡

建议实现纯函数：

```ts
type LayoutNode = ProxyCanvasNode & {
  width: number;
  height: number;
  x: number;
  y: number;
  subtreeHeight: number;
};

function computeProxyCanvasLayout(input: {
  listeners: ProxyListener[];
  routesByListener: Map<string, ProxyRoute[]>;
  upstreamsByRoute: Map<string, ProxyUpstream[]>;
  runtime: ProxyRuntimeSnapshot;
}): {
  nodes: LayoutNode[];
  edges: ProxyCanvasEdge[];
  bounds: CanvasBounds;
};
```

布局步骤：

1. 构建 Listener 树。
2. 按业务优先级排序 Route。
3. 按业务优先级排序 Upstream。
4. 自底向上计算每个子树高度。
5. 自顶向下分配 `x/y`。
6. 对每个 Listener 子树之间保留较大垂直间距。
7. 输出节点、边、整体 bounds。

建议默认尺寸：

```ts
const NODE_WIDTH = 190;
const NODE_HEIGHT = 72;
const LAYER_GAP = 230;
const SIBLING_GAP = 28;
const TREE_GAP = 88;
const CANVAS_PADDING = 80;
```

### 7.4 布局动画

每次数据或布局变化时：

- 根据 node key 复用 Pixi display object
- 新节点从父节点附近淡入并移动到目标位置
- 删除节点淡出后移除
- 已存在节点从旧位置缓动到新位置

动画参数：

```ts
duration: 180ms - 240ms
easing: easeOutCubic
```

动画只影响视图，不改变业务数据。

## 8. PixiJS 渲染架构

### 8.1 依赖

新增依赖：

```bash
pnpm add pixi.js
```

要求：

- 使用最新稳定版 PixiJS
- 使用最新 API 初始化 `Application`
- 不使用过时的 v6/v7 写法

具体版本以开发时 `pnpm add pixi.js` 安装结果为准。

### 8.2 组件拆分

建议新增目录：

```text
src/features/proxy/canvas/
├─ ProxyCanvas.tsx
├─ ProxyCanvasToolbar.tsx
├─ ProxyCanvasDetailPanel.tsx
├─ ProxyCanvasContextMenu.tsx
├─ layout.ts
├─ model.ts
├─ pixiRenderer.ts
├─ interactions.ts
├─ search.ts
└─ styles.css
```

职责：

- `ProxyCanvas.tsx`
  - Solid 容器组件
  - 管理 Pixi 生命周期
  - 接收 Proxy 数据与事件回调
- `ProxyCanvasToolbar.tsx`
  - 搜索、刷新、新建 Listener、回到内容区、缩放状态
- `ProxyCanvasDetailPanel.tsx`
  - 选中节点详情展示
- `ProxyCanvasContextMenu.tsx`
  - 自定义上下文菜单
- `layout.ts`
  - 纯布局算法，可单元测试
- `model.ts`
  - 数据归一化、节点/边模型
- `pixiRenderer.ts`
  - Pixi display object 创建、更新、销毁
- `interactions.ts`
  - 平移、缩放、点击、右键菜单
- `search.ts`
  - 搜索索引、匹配、定位目标计算

### 8.3 Pixi 容器层级

```text
Application.stage
└─ viewportContainer
   ├─ edgeLayer
   ├─ nodeLayer
   └─ overlayLayer
```

说明：

- `viewportContainer` 承载缩放和平移。
- `edgeLayer` 先绘制，避免覆盖节点。
- `nodeLayer` 绘制节点卡片。
- `overlayLayer` 绘制选中框、搜索定位高亮等。
- Context Menu 使用 DOM 实现，定位来自 Pixi 交互事件的屏幕坐标。

### 8.4 渲染性能策略

- 节点和边按 key 增量更新，不全量销毁重建
- 静态节点背景使用 `Graphics` 绘制
- 文本使用 `Text`，样式对象复用
- 缩放/平移只更新 `viewportContainer.scale/position`
- 只有数据变化或状态变化时重绘节点和边
- `ResizeObserver` 监听容器尺寸变化并 resize renderer
- 组件卸载时销毁 Pixi Application、事件监听、动画帧

## 9. 画布交互设计

### 9.1 平移

- 鼠标左键拖拽空白区域平移
- 拖拽节点不改变节点位置
- 拖拽距离超过阈值后不触发节点点击

### 9.2 缩放

- 鼠标滚轮缩放
- 缩放范围：`0.4 - 3.0`
- 以鼠标位置为缩放中心
- Toolbar 显示当前缩放比例

### 9.3 回到内容区

当主要内容区被平移到画布外时显示 `回到内容区` 按钮。

判定方式：

- 根据 layout bounds 转换到屏幕坐标
- 如果 bounds 与 canvas viewport 没有交集，或可见面积低于阈值，显示按钮

点击后：

- reset scale 为 `1`
- 平移到内容 bounds 左上角或居中位置
- 清理搜索定位临时高亮

### 9.4 节点选择

- 单击节点选中
- 选中节点显示描边
- 画布下方详情面板显示该节点完整信息
- 单击空白区域取消选中
- 数据刷新后，如果原选中节点仍存在，保留选中；否则清空

### 9.5 上下文菜单

使用自定义 DOM 菜单，不使用浏览器原生菜单。

空白区域菜单：

- 新建 Listener

Listener 节点菜单：

- 编辑 Listener
- 创建下游 Route
- 删除 Listener

Route 节点菜单：

- 编辑 Route
- 创建下游 Upstream
- 删除 Route

Upstream 节点菜单：

- 编辑 Upstream
- 删除 Upstream

菜单行为：

- 右键打开
- 点击菜单项后关闭
- 点击画布空白、滚动、缩放、窗口 resize 后关闭
- 菜单越界时自动反向展开

## 10. 搜索与定位

### 10.1 入口

- `Ctrl + F` 唤起/关闭搜索
- Toolbar 提供搜索框
- 搜索打开时自动聚焦输入框
- `Esc` 关闭搜索或清空搜索状态

### 10.2 搜索范围

Listener：

- name
- protocol
- listen_host
- listen_port

Route：

- server_names
- path_prefix
- default route 文案

Upstream：

- target_kind
- target_ref
- target_host
- target_port
- upstream_scheme
- path rewrite

### 10.3 搜索结果

- 支持上一个 / 下一个
- 显示 `current / total`
- 定位时选中节点
- 视图平移到节点可见区域
- 节点短暂高亮

### 10.4 定位策略

- 如果当前缩放过小，可临时恢复到不小于 `0.9`
- 保持缩放不超过 `1.2`
- 目标节点尽量定位在画布中心偏左，保留右侧下游阅读空间

## 11. 详情面板

详情面板显示在画布下方，仅当有选中节点时展示。

Listener 详情：

- 名称
- 监听地址
- 协议
- TLS 模式
- 证书
- 绑定模式
- 网卡
- enabled
- runtime state
- last error

Route 详情：

- server names
- path prefix
- default route
- enabled
- hit count
- error count
- last server name
- last request path
- last error

Upstream 详情：

- target kind
- target ref
- target host
- target port
- upstream scheme
- rewrite from/to
- enabled
- hit count
- error count
- last target
- last request path
- last error

详情面板操作：

- 编辑当前节点
- 删除当前节点
- 对 Listener：创建 Route
- 对 Route：创建 Upstream

## 12. 删除确认与级联提示

删除统一走二次确认 Modal。

Listener 删除：

- 提示将级联删除该 Listener 下所有 Route 与 Upstream
- 显示影响数量：
  - Route 数
  - Upstream 数

Route 删除：

- 提示将级联删除该 Route 下所有 Upstream
- 显示影响 Upstream 数

Upstream 删除：

- 提示仅删除当前 Upstream

实现方式：

- 前端根据当前已加载数据计算影响范围
- Modal 文案明确对象名称和级联数量
- 删除后 refetch 数据并重算布局

## 13. 与现有 ProxyPage 集成

### 13.1 保留现有状态

当前 `ProxyPage.tsx` 已有：

- listeners query
- certificates query
- routes query
- upstreams query
- runtime query
- route runtime query
- upstream runtime query
- editingTarget
- deleteTarget
- 表单草稿 signal
- handleSaveListener / handleSaveRoute / handleSaveUpstream / handleSaveCertificate

改造原则：

- 不重写后端 API
- 不重写表单校验逻辑
- 不重写 Modal 表单
- 将原三段列表渲染替换为 `ProxyCanvasSection`
- Canvas 通过回调触发已有打开 Modal 的函数

### 13.2 数据加载调整

当前 routes/upstreams query 依赖选中的 Listener/Route，只加载单个分支。

画布需要完整拓扑，因此需要调整为加载所有 Listener 下的 Route 与所有 Route 下的 Upstream。

推荐方案：

- 前端新增组合 query：`proxyTopologyQuery`
- 先加载 listeners
- 并发加载每个 listener 的 routes
- 并发加载每个 route 的 upstreams
- 并发/复用 runtime 数据
- 输出完整拓扑快照

可选后端优化：

- 后续如果性能不足，再增加后端 `get_proxy_topology` 聚合命令
- 当前阶段优先前端聚合，降低后端改动风险

### 13.3 选中状态变化

旧逻辑：

- `selectedListenerId`
- `selectedRouteId`

新逻辑：

```ts
type SelectedProxyNode =
  | { kind: "listener"; id: string }
  | { kind: "route"; id: string }
  | { kind: "upstream"; id: string }
  | null;
```

兼容策略：

- Canvas 选中 Listener 时同步 `selectedListenerId`
- Canvas 选中 Route 时同步 `selectedListenerId` 和 `selectedRouteId`
- Canvas 选中 Upstream 时同步其 parent route/listener
- 这样可以最大化复用已有 Modal 默认值和查询逻辑

## 14. 视图状态保存

保存范围：

- 当前应用运行期间
- Tab 切换恢复
- 应用重启后不恢复

保存内容：

```ts
type ProxyCanvasViewState = {
  scale: number;
  x: number;
  y: number;
  selectedNode: SelectedProxyNode;
  searchOpen: boolean;
  searchKeyword: string;
};
```

实现位置：

- 前端模块级 store 或 Solid store
- 不写 localStorage
- 不写数据库

## 15. 开发任务拆分

### 阶段 1：依赖与基础骨架

- 新增 `pixi.js`
- 新建 `src/features/proxy/canvas/`
- 抽出 `ProxyCanvasSection`
- 在 Proxy 页面中用空画布占位替换三段列表
- 保留现有证书区、Modal、迁移提示、Metric

验收：

- 页面可打开
- Pixi canvas 正常初始化/销毁
- 窗口 resize 不报错
- `pnpm typecheck` 通过

### 阶段 2：拓扑数据聚合与布局算法

- 实现完整 topology query
- 实现 `model.ts`
- 实现 `layout.ts`
- 编写布局纯函数测试
- 按业务优先级排序

验收：

- 多 Listener、多 Route、多 Upstream 数据可生成稳定节点/边
- 新增节点后布局稳定
- 单元测试覆盖排序和 bounds

### 阶段 3：Pixi 节点与链路渲染

- 绘制 Listener / Route / Upstream 节点
- 绘制曲线或折线边
- 绘制 enabled/disabled/error 状态
- 增量更新 display objects
- 实现布局过渡动画

验收：

- 节点和边展示符合原型方向
- 禁用态灰色
- 错误态红色
- 数据刷新不会闪烁式全量重建

### 阶段 4：平移、缩放、回到内容区

- 实现拖拽平移
- 实现鼠标滚轮缩放
- 限制缩放范围 `0.4 - 3.0`
- 实现内容区可见性检测
- 实现 `回到内容区`
- 实现运行期视图状态保存

验收：

- 平移缩放流畅
- 内容被移出后显示回到内容区
- 点击后恢复可见内容
- Tab 切换后视图状态恢复

### 阶段 5：选择、详情面板、上下文菜单

- 节点点击选中
- 空白点击取消选中
- 详情面板展示完整字段
- 空白处上下文菜单
- 节点上下文菜单
- 菜单项接入现有 Modal 打开逻辑

验收：

- 所有节点类型可选中并展示详情
- 所有新增/编辑入口可用
- 右键菜单不触发浏览器默认菜单
- 菜单越界可处理

### 阶段 6：搜索与定位

- `Ctrl + F` 打开/关闭搜索
- 构建搜索索引
- 上一条 / 下一条
- 定位并选中节点
- 搜索高亮

验收：

- Listener / Route / Upstream 均可搜索
- 多结果可切换
- 定位后节点可见
- `Esc` 行为符合预期

### 阶段 7：删除确认与级联提示

- Listener 删除级联提示
- Route 删除级联提示
- Upstream 删除提示
- 接入现有 delete API
- 删除后 refetch 并重算布局

验收：

- 删除前明确影响范围
- 删除后画布自动更新
- 删除选中节点后详情面板清空

### 阶段 8：样式收敛与回归

- 对齐现有 UI 视觉语言
- 优化节点色彩、线条、字体、hover/selected 状态
- 确认 Windows 桌面窗口缩放适配
- 补齐 i18n 文案
- 更新开发日志

验收：

- `pnpm typecheck`
- `pnpm build`
- 如有纯函数测试，执行对应测试
- 现有 Rust 测试无需因纯前端改造重复跑；若改后端 API，则补跑相关 cargo tests

## 16. 风险与应对

### 16.1 PixiJS 与 Solid 生命周期耦合风险

风险：

- 重复初始化 Application
- 事件监听残留
- 组件卸载后动画帧继续运行

应对：

- Pixi 初始化只放在 `onMount`
- 所有 watcher/event/ticker 在 `onCleanup` 清理
- renderer 逻辑封装为可显式 `destroy()` 的对象

### 16.2 完整拓扑查询导致请求数量增加

风险：

- Listener/Route 数量大时请求增多

应对：

- 第一阶段前端并发聚合
- 使用 TanStack Query 缓存
- 后续如出现性能瓶颈，再加后端聚合命令

### 16.3 Canvas 可访问性弱

风险：

- Canvas 内文本不可被系统辅助工具直接读取

应对：

- Toolbar 与详情面板使用 DOM
- 搜索、详情和菜单提供可访问文本
- 节点信息在详情面板中完整呈现

### 16.4 布局跳动

风险：

- 数据变化后节点大幅跳动影响理解

应对：

- 使用稳定排序
- 新增节点只插入对应子树排序位置
- 通过动画缓动过渡

### 16.5 上下文菜单与 Modal 状态冲突

风险：

- 右键菜单打开后数据刷新或选中变化导致菜单目标失效

应对：

- 菜单打开时保存目标 key
- 执行动作前校验目标仍存在
- 目标失效则关闭菜单并提示

## 17. 自动化验收清单

- `layout.ts`：
  - 多 Listener 纵向排列
  - Route 按业务优先级排序
  - Upstream 按业务优先级排序
  - bounds 计算正确
  - 新增节点后已有节点 key 稳定
- `search.ts`：
  - Listener 名称/地址可搜索
  - Route server name/path 可搜索
  - Upstream target 可搜索
  - 多结果 next/prev 正确
- `model.ts`：
  - runtime error 正确映射到节点和边
  - disabled 状态正确映射到节点和边
  - 级联删除影响数量计算正确
- 构建：
  - `pnpm typecheck`
  - `pnpm build`

## 18. 人工验收清单

### 18.1 基础渲染

1. 打开 Proxy 页面。
2. 确认 Metric Cards 正常显示。
3. 确认迁移提示位于画布上方。
4. 确认证书管理区域仍在画布下方或页面后续区域。
5. 确认 Listener / Route / Upstream 三段列表不再出现。

### 18.2 画布布局

1. 创建两个 Listener。
2. 给第一个 Listener 创建多条 Route。
3. 给某条 Route 创建多个 Upstream。
4. 确认布局为 `Listener -> Route -> Upstream[]`。
5. 确认多个 Listener 纵向排列。
6. 确认新增节点后自动重新排布。
7. 确认布局变化有过渡动画。

### 18.3 交互

1. 拖拽空白画布，确认可以平移。
2. 鼠标滚轮缩放，确认范围限制在 `0.4 - 3.0`。
3. 将内容区移出视图，确认出现 `回到内容区`。
4. 点击 `回到内容区`，确认内容恢复可见。
5. 单击节点，确认详情面板出现。
6. 单击空白，确认详情面板隐藏。

### 18.4 上下文菜单

1. 右键空白区域，确认出现新建 Listener。
2. 右键 Listener，确认出现编辑、创建 Route、删除。
3. 右键 Route，确认出现编辑、创建 Upstream、删除。
4. 右键 Upstream，确认出现编辑、删除。
5. 确认右键不会弹出浏览器原生菜单。
6. 确认菜单靠近窗口边缘时不溢出。

### 18.5 搜索

1. 按 `Ctrl + F`，确认搜索框打开并聚焦。
2. 搜索 Listener 名称，确认能定位。
3. 搜索 Route server name/path，确认能定位。
4. 搜索 Upstream target，确认能定位。
5. 多结果时切换上一个/下一个。
6. 按 `Esc` 关闭搜索。

### 18.6 状态展示

1. 禁用 Listener / Route / Upstream，确认节点灰色展示。
2. 制造 Listener 运行错误，确认 Listener 红色高亮。
3. 制造 Route 错误，确认 Route 和 `Listener -> Route` 边红色高亮。
4. 制造 Upstream 错误，确认 Upstream 和 `Route -> Upstream` 边红色高亮。

### 18.7 删除确认

1. 删除 Upstream，确认二次确认弹窗只提示当前节点。
2. 删除 Route，确认弹窗提示将级联删除 Upstream 数量。
3. 删除 Listener，确认弹窗提示将级联删除 Route 和 Upstream 数量。
4. 确认删除后画布自动刷新。

## 19. 收敛标准

本次优化完成的判定标准：

- PixiJS 画布替代原三段列表
- 节点/链路完整表达 Proxy 拓扑
- 新增、编辑、删除入口完整可用
- 平移、缩放、回到内容区可用
- 自动布局与动画可用
- 搜索定位可用
- 详情面板可用
- 禁用态和错误态可视化可用
- 证书管理区域保持可用
- `pnpm typecheck` 通过
- `pnpm build` 通过
- 开发日志更新完成
