# 三期任务拆解清单

## 1. 文档信息

- 项目名称：`wsl-bridge`
- 关联设计文档：`docs/phase3-proxy-hosts-design.md`
- 文档版本：`v1.0`
- 更新时间：`2026-05-15`
- 目标读者：产品、研发、测试

## 2. 拆解原则

三期按五个里程碑推进：

1. `M6.1 Hosts`
2. `M6.2 Proxy HTTP`
3. `M6.3 Proxy HTTPS`
4. `M6.4 WebSocket / gRPC / 观测`
5. `M6.5 Legacy 迁移与收尾`

拆解原则：

- 每个里程碑都按“后端 / 前端 / 测试 / 文档”拆分
- 每个任务应尽量对应明确文件边界或模块边界
- 优先先落数据模型与 command，再落页面与交互
- 高风险能力单独设验证任务，不与普通 UI 开发混在一起

## 3. 里程碑总览

| 里程碑 | 目标 | 预计工时 | 关键交付物 |
|------|------|------|------|
| `M6.1` | Hosts 模块可用 | 4-5 天 | Hosts 数据模型、系统文件读写、Hosts 页面 |
| `M6.2` | Proxy HTTP 主链路 | 6-8 天 | Proxy 数据模型、HTTP listener/route/upstream、Proxy 页面 |
| `M6.3` | Proxy HTTPS 主链路 | 6-8 天 | TLS 终止、证书管理、本地 CA 引导 |
| `M6.4` | 高级协议与观测 | 5-7 天 | WebSocket、gRPC、Proxy 运行态与日志 |
| `M6.5` | Legacy 迁移与收尾 | 4-6 天 | 迁移向导、Rules legacy 限制、回滚与验收 |

## 4. M6.1 Hosts 模块

### 4.1 后端任务

#### A. 数据模型与存储

1. 在共享类型层新增 Hosts DTO
   - `HostsGroup`
   - `HostsEntry`
   - `CreateHostsGroupRequest`
   - `UpdateHostsGroupRequest`
   - `SaveHostsEntriesRequest`
2. 在 SQLite Store 中新增表结构
   - `hosts_group`
   - `hosts_entry`
3. 为 Hosts 表增加加载/保存逻辑
4. 为快照机制补齐 Hosts 数据持久化

#### B. 系统 Hosts 文件能力

1. 新增系统 hosts 路径解析工具
2. 新增 hosts 文本解析器
   - 支持 IPv4
   - 支持 IPv6
   - 支持尾部注释
   - 支持空行与注释行跳过
3. 新增 hosts 文本渲染器
4. 新增系统 hosts 文件读写封装
5. 增加“整体写入当前分组”的原子写入策略
   - 先写临时文件
   - 成功后替换原文件

#### C. 业务编排

1. 新增 `bootstrap_default_hosts_group`
   - 数据库无分组时，从系统 hosts 导入 `default`
2. 新增分组 CRUD 能力
3. 新增分组复制能力
4. 新增分组导入/导出能力
5. 新增“设为生效”能力
   - 激活前校验分组可用
   - 写入系统 hosts
   - 清理其他分组 `is_active`

#### D. Tauri Commands

1. 新增 Hosts command 定义
2. 在 `tauri_commands.rs` 注册：
   - `list_hosts_groups`
   - `create_hosts_group`
   - `update_hosts_group`
   - `delete_hosts_group`
   - `copy_hosts_group`
   - `list_hosts_entries`
   - `save_hosts_entries`
   - `import_hosts_group_from_file`
   - `export_hosts_group_to_file`
   - `activate_hosts_group`
   - `bootstrap_default_hosts_group`

### 4.2 前端任务

#### A. 路由与页面骨架

1. 新增 `Hosts` 路由
2. 将 `Hosts` 加入顶层 Tab 导航
3. 创建 `src/features/hosts/HostsPage.tsx`
4. 创建 `src/features/hosts/api.ts`

#### B. 页面结构

1. 左侧分组列表区
2. 右侧记录表格区
3. 顶部权限横幅
4. 顶部操作栏

#### C. 分组交互

1. 新建分组弹窗
2. 复制分组操作
3. 删除分组确认
4. 导入文件操作
5. 导出文件操作
6. “设为生效”按钮

#### D. 表格交互

1. 表格列定义
   - `enabled`
   - `ip`
   - `domain`
   - `comment`
   - `actions`
2. 行内编辑
3. 新增行
4. 删除行
5. 批量启用/禁用
6. 保存当前分组

#### E. 前端校验

1. IP 格式校验
2. 域名非空校验
3. 分组名非空校验
4. 非管理员态禁用写操作

### 4.3 测试任务

1. Rust 单元测试
   - hosts 文本解析
   - hosts 文本渲染
   - default 分组导入
   - 分组复制
   - 分组激活切换
2. 前端交互验证
   - 分组切换
   - 表格保存
   - 非管理员禁用态
3. 手工验证
   - 从系统 hosts 初始化
   - 导入 / 导出一致性
   - 激活后系统文件正确写入

### 4.4 验收点

1. 首次进入页面时可自动生成 `default` 分组
2. 分组 CRUD、复制、导入、导出均可用
3. 仅允许一个分组生效
4. 非管理员模式下页面可见但不可写系统文件

## 5. M6.2 Proxy HTTP 主链路

### 5.1 后端任务

#### A. 数据模型与存储

1. 在共享类型层新增 Proxy DTO
   - `ProxyListener`
   - `ProxyRoute`
   - `ProxyUpstream`
   - `ProxyCertificate`
   - `CreateProxyListenerRequest`
   - `CreateProxyRouteRequest`
   - `CreateProxyUpstreamRequest`
2. 在 SQLite 中新增表
   - `proxy_listener`
   - `proxy_route`
   - `proxy_upstream`
   - `proxy_certificate`
3. 增加 Proxy 数据持久化与加载逻辑

#### B. 匹配引擎

1. 实现 listener 装配器
2. 实现 route 匹配器
   - 精确域名
   - 默认路由
   - 同冲突倒序匹配
3. 实现 path prefix 匹配
4. 实现 path prefix 改写
5. 实现 route 有效性校验

#### C. 上游解析

1. 复用现有 WSL 目标解析
2. 复用现有 Hyper-V 目标解析
3. 支持 Static 上游
4. 支持 `http` 上游请求转发

#### D. HTTP 代理执行器

1. 新建独立 Proxy Runtime 模块
2. 实现 HTTP listener
3. 实现按 Host + Path 路由
4. 实现请求转发
5. 实现响应回传
6. 实现失败回退到默认路由
7. 默认路由失败时返回错误并记日志

#### E. Commands

1. 新增 Proxy command
2. 注册：
   - `list_proxy_listeners`
   - `create_proxy_listener`
   - `update_proxy_listener`
   - `delete_proxy_listener`
   - `list_proxy_routes`
   - `create_proxy_route`
   - `update_proxy_route`
   - `delete_proxy_route`
   - `list_proxy_upstreams`
   - `create_proxy_upstream`
   - `update_proxy_upstream`
   - `delete_proxy_upstream`
   - `apply_proxy`
   - `stop_proxy`

### 5.2 前端任务

#### A. 路由与页面

1. 新增 `Proxy` 路由
2. 将 `Proxy` 加入主导航
3. 创建 `src/features/proxy/ProxyPage.tsx`
4. 创建 `src/features/proxy/api.ts`

#### B. 页面结构

1. Listener 列表区
2. Route 编辑区
3. Upstream 编辑区
4. 运行状态与调试输出区

#### C. Listener 管理

1. 新建 Listener 弹窗
2. 默认协议为 HTTP
3. 默认隐藏 HTTPS 配置区
4. 支持编辑 `listen_host / listen_port / bind_mode / nic_id`

#### D. Route 管理

1. 新建 Route
2. 编辑 `server_names`
3. 编辑 `path_prefix`
4. 标记默认路由
5. 启用/禁用 Route

#### E. Upstream 管理

1. 选择 `wsl / hyperv / static`
2. 选择 / 输入目标地址
3. 配置 `path prefix` 改写
4. 配置上游 scheme

#### F. 前端校验

1. 默认路由唯一性校验
2. `server_name` 非法输入校验
3. `path_prefix` 必须以 `/` 开头
4. 上游配置完整性校验

### 5.3 测试任务

1. Rust 单元测试
   - route 匹配优先级
   - 默认路由回退
   - path prefix 改写
   - WSL / Hyper-V / Static 上游解析
2. 手工联调
   - `a.com` / `b.com` 分流
   - 默认路由命中
   - 路径改写是否生效
3. 前端验证
   - 新建 listener 默认是 HTTP
   - Route / Upstream 联动保存

### 5.4 验收点

1. 可按 `server_name` 正确分流 HTTP 请求
2. 默认路由可兜底未命中请求
3. 支持 `path prefix` 改写
4. 新建 Proxy 规则默认走 HTTP

## 6. M6.3 Proxy HTTPS 主链路

### 6.1 后端任务

#### A. TLS Runtime

1. 引入 TLS 终止执行链路
2. 支持 HTTPS listener
3. 支持按 listener 绑定证书
4. 支持 TLS 握手失败日志

#### B. 证书管理

1. 证书上传存储
2. 证书与私钥匹配校验
3. 证书域名解析
4. 证书删除保护
   - 正在被 listener 使用时不可删除

#### C. 本地 CA 引导

1. 生成本地 CA
2. 生成指定域名证书
3. 提供安装引导状态
4. 提供本地 CA 元数据存储

#### D. 通配符匹配

1. 实现精确匹配
2. 实现 `*.example.com`
3. 实现 `.example.com`
4. 增加冲突检测与排序

### 6.2 前端任务

#### A. HTTPS 配置区

1. 在 Listener 表单中增加协议切换
2. 仅用户切换到 HTTPS 时显示 TLS 配置
3. 提供：
   - 手动上传证书
   - 本地 CA 引导生成
   - 证书绑定选择

#### B. 证书页面或子面板

1. 证书列表
2. 上传入口
3. 本地 CA 生成向导
4. 证书使用关系提示

#### C. 通配符 UX

1. 提示支持 `*.example.com`
2. 提示支持 `.example.com`
3. 输入非法时给出即时校验

### 6.3 测试任务

1. Rust 单元测试
   - TLS listener 建立
   - 证书装载
   - 通配符匹配
2. 手工验证
   - 浏览器访问 HTTPS
   - 手动证书场景
   - 本地 CA 场景
3. 异常验证
   - 证书与域名不匹配
   - 证书缺失
   - 私钥不匹配

### 6.4 验收点

1. HTTPS listener 可正常建立
2. 手动上传证书可用
3. 本地 CA 引导链路可用
4. 通配符匹配符合预期

## 7. M6.4 WebSocket / gRPC / 观测

### 7.1 后端任务

#### A. WebSocket

1. 检测 Upgrade 请求
2. 建立双向转发
3. 记录连接建立与关闭日志

#### B. gRPC

1. 增加 HTTP/2 上游转发
2. 支持 gRPC 基础透传
3. 区分 `grpc / grpcs`

#### C. 观测性

1. 增加 Proxy 运行态模型
2. 增加 listener / route / upstream 级日志字段
3. 增加错误分类
   - TLS 错误
   - route 未命中
   - upstream 连接失败
   - upgrade 失败
4. 增加 Proxy runtime 状态查询 command

### 7.2 前端任务

1. Proxy 运行态面板
2. 错误摘要区
3. route / upstream 命中信息展示
4. 高级协议标识
   - WebSocket
   - gRPC

### 7.3 测试任务

1. WebSocket 回环联调
2. gRPC Demo 服务联调
3. Proxy 日志字段完整性检查
4. Runtime 状态刷新验证

### 7.4 验收点

1. WebSocket 主路径可用
2. gRPC 主路径可用
3. 运行态与日志可定位到 route / upstream 级别

## 8. M6.5 Legacy 迁移与收尾

### 8.1 后端任务

#### A. Legacy 标记能力

1. 为旧规则增加迁移状态字段或迁移映射表
2. 支持 `pending / migrated / rollbacked`

#### B. 迁移逻辑

1. 实现 `tcp_fwd -> Proxy` 自动迁移
2. 实现 `http_proxy -> Proxy` 自动迁移
3. `http_proxy` 迁移默认值：
   - `server_names = ["127.0.0.1", "localhost"]`
   - HTTPS 关闭
   - 无路径改写
4. 迁移后禁用旧规则
5. 保留回滚能力

#### C. Rules 限制收口

1. `Rules` 禁止新增 `tcp_fwd / http_proxy`
2. 仅允许新增 `udp_fwd / socks5_proxy`

### 8.2 前端任务

#### A. 迁移引导

1. 首次进入 Proxy 的迁移横幅
2. Rules 列表中的“迁移到 Proxy”按钮
3. 迁移预览弹窗
4. 迁移结果提示

#### B. Legacy 状态展示

1. 规则类型标签
2. `待迁移 / 已迁移 / Legacy` 状态展示
3. 已迁移规则禁用态样式

#### C. 新增限制

1. Rules 新建表单中移除 `tcp_fwd / http_proxy`
2. 旧类型显示只读提示或迁移提示

### 8.3 测试任务

1. 自动迁移结果核对
2. 回滚流程验证
3. Rules legacy 限制验证
4. 迁移审计日志验证

### 8.4 验收点

1. `tcp_fwd / http_proxy` 可迁入 Proxy
2. 迁移后旧规则被禁用但仍可回滚
3. Rules 不再允许新增 `tcp_fwd / http_proxy`

## 9. 并行机会

可并行开发的工作包：

1. `M6.1`
   - Hosts 后端存储
   - Hosts 前端页面骨架
2. `M6.2`
   - Proxy SQLite 与 DTO
   - Proxy 页面结构与表单
3. `M6.3`
   - 证书上传管理
   - 本地 CA 引导 UI
4. `M6.5`
   - Legacy 状态展示
   - 迁移逻辑实现

不建议并行的工作包：

1. HTTP runtime 与 HTTPS runtime 核心执行链路
2. gRPC 执行链路与 Proxy 日志模型重构

## 10. 开发顺序建议

推荐执行顺序：

1. 先做 `M6.1 Hosts`
2. 再做 `M6.2 Proxy HTTP`
3. 然后做 `M6.3 Proxy HTTPS`
4. 再做 `M6.5 Legacy 迁移`
5. 最后补 `M6.4 WebSocket / gRPC / 观测`

说明：

- 虽然里程碑编号把 `M6.4` 放在 `M6.5` 前，但从项目收敛角度看，迁移向导可早于 gRPC 完整支持落地。
- 如果业务要求优先替代旧入口，也可将 `M6.5` 提前到 HTTPS 基础能力之后。

## 11. 每阶段交付检查清单

### 11.1 开发前

1. 明确 DTO 与表结构
2. 明确 command 与前端 API
3. 明确验收数据样例

### 11.2 开发中

1. 同步更新 i18n
2. 同步补充错误日志
3. 同步更新开发日志
4. 复杂流程先写最小可测链路

### 11.3 阶段收尾

1. `pnpm typecheck`
2. Rust 测试通过
3. 手工联调通过
4. 文档与开发日志更新

## 12. 收敛补记（截至 2026-05-15）

### 12.1 Proxy 收敛结论

Proxy 模块按三期当前口径已收敛：

- 功能主链路可用
- 自动化测试面已覆盖 HTTP / HTTPS / WS / WSS / gRPC 约束、运行态与 TLS 信任链
- `Rules -> Proxy` 迁移、回滚与 legacy 限制已落地

当前验证基线：

- `pnpm typecheck`
- `cargo test --manifest-path src-tauri/crates/core/Cargo.toml`
- `cargo test --manifest-path src-tauri/app/Cargo.toml`

### 12.2 三期债务项

以下项不再阻塞 Proxy 模块收敛，但需在后续版本继续跟踪：

1. `grpcs` 的 Windows 本地稳定端到端自动化回放
   - 当前问题：`TLS client -> HTTPS Listener -> grpcs TLS upstream` 回放链路仍可能出现 `10053`
   - 当前状态：功能链路已接入，稳定非 E2E 自动化已具备
   - 后续建议：单独评估更稳定的 HTTPS 本地回放方案，再补强 E2E
