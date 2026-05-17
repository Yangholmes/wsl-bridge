# 三期验收清单

## 1. 文档信息

- 项目名称：`wsl-bridge`
- 关联文档：
  - `docs/phase3-proxy-hosts-design.md`
  - `docs/phase3-task-breakdown.md`
- 文档版本：`v1.0`
- 更新时间：`2026-05-16`
- 适用范围：三期 `Hosts + Proxy + Legacy Rules`

## 2. 验收口径

三期验收分为两部分：

1. **自动化验收**
   - 以现有 Rust / TypeScript 自动化测试与本轮补充测试为准
   - 对照三期设计文档的正式验收标准逐项判定
   - 对无法稳定自动化的项，明确记录为债务或人工验收项
2. **人工验收**
   - 面向桌面应用真实操作路径
   - 提供逐步操作指南
   - 适用于发布前联调、测试回归与产品验收

自动化验收状态定义：

- `通过`：已有稳定自动化覆盖，且本轮验证通过
- `债务`：功能已接入，但稳定自动化覆盖暂不成立，已登记为技术债务
- `不符合预期`：本轮自动化验证失败或缺少必要覆盖，需列入验收结果

## 3. 自动化验收

### 3.1 Hosts

| 编号 | 验收项 | 预期 | 自动化结果 | 备注 |
|------|------|------|------|------|
| H-01 | 首次进入 `Hosts` 页面时，可将系统 `hosts` 文件导入为 `default` 分组 | 后端可完成 bootstrap，生成 `default` 分组与结构化记录 | 通过 | `hosts_bootstrap_save_copy_and_activate_work` |
| H-02 | 用户可创建、复制、删除、导入、导出 hosts 分组 | 对应 command 与持久化行为可自动验证 | 通过 | `hosts_bootstrap_save_copy_and_activate_work` + `hosts_import_export_delete_and_validation_work` |
| H-03 | 用户可通过表格编辑每条 `ip / domain / comment / enabled` 记录 | 保存后可正确读回，非法输入被拒绝 | 通过 | 保存读回与非法 IP 拒绝已覆盖 |
| H-04 | 仅一个分组可处于生效状态 | 激活新分组后，旧分组自动失活 | 通过 | `hosts_import_export_delete_and_validation_work` |
| H-05 | 生效后系统 `hosts` 文件内容与分组导出结果一致 | 写入结果与导出文本一致 | 通过 | `activate` 与 `export` 共用 `render_hosts_text`，本轮组合验证通过 |
| H-06 | 非管理员模式下不隐藏 Tab，但禁止写入系统文件 | 主要依赖人工验收；若有静态前端约束可附带说明 | 债务 | 当前未补稳定自动化，保留人工验收 |

### 3.2 Proxy

| 编号 | 验收项 | 预期 | 自动化结果 | 备注 |
|------|------|------|------|------|
| P-01 | 可创建 HTTP listener，并按 `server_name` 正确分流到不同上游 | Host 匹配与转发正确 | 通过 | `proxy_http_listener_routes_and_rewrites_path` + `proxy_runtime` 路由选择单测 |
| P-02 | 默认未配置 `server_name` 的路由可接收未命中流量 | 默认路由兜底生效 | 通过 | `default_route_matches_when_no_server_name_matches` |
| P-03 | 域名冲突按创建时间倒序匹配，最新规则优先 | 匹配优先级稳定 | 通过 | `newer_route_wins_for_same_match_class` |
| P-04 | 支持 `*.example.com` 和 `.example.com` 匹配 | 通配符语义符合设计 | 通过 | `exact_match_beats_wildcard_and_default` + `dot_prefix_matches_root_and_subdomain` |
| P-05 | 支持 `path prefix` 改写 | 上游收到改写后的路径 | 通过 | `rewrite_path_rewrites_prefix_only` + `proxy_http_listener_routes_and_rewrites_path` |
| P-06 | 支持 HTTPS TLS 终止 | HTTPS listener / 证书装载 / 本地 CA 基础链路可用 | 通过 | 证书绑定、`manual_cert`、`local_ca`、HTTPS listener 运行态测试已通过 |
| P-07 | 支持 WebSocket 与 gRPC 主路径 | `ws / wss / grpc` 自动化通过；`grpcs` 运行时已接入 | 通过 | `ws / wss / grpc(h2c) / grpcs` 已自动化通过，`grpcs` 由 `proxy_https_listener_tunnels_grpcs_prior_knowledge` 覆盖 |
| P-08 | 默认路由异常时，应用拒绝请求并写错误日志 | 未命中或上游不可用时正确返回错误并记日志/指标 | 通过 | 以 `https / wss` 未受信上游回归证明 `502 + route/upstream error metrics`，错误日志写入逻辑同分支接入 |

### 3.3 Legacy Rules

| 编号 | 验收项 | 预期 | 自动化结果 | 备注 |
|------|------|------|------|------|
| L-01 | 旧 `Rules` 保留且可继续管理 `udp_fwd / socks5_proxy` | 旧模块功能仍可用 | 通过 | `udp_forwarding_works` + `socks5_connect_works` |
| L-02 | 不允许新增 `tcp_fwd / http_proxy` | 新建入口与后端限制收口 | 不符合预期 | 前端创建表单已限制为 `udp_fwd / socks5_proxy`，但后端 `create_rule` 仍接受 `tcp_fwd / http_proxy` |
| L-03 | 可对旧 `tcp_fwd / http_proxy` 发起迁移 | 自动迁移结果符合设计 | 通过 | `tcp_rule_can_migrate_to_proxy` + `http_proxy_rule_migrates_as_proxy_draft` |
| L-04 | 迁移后自动生成 Proxy 配置并保留旧规则记录 | listener / route / upstream 与 migration record 正确写入 | 通过 | 两条迁移测试已覆盖 listener / route / upstream / migration record |
| L-05 | 迁移结果支持审计与回滚 | rollback 后 Proxy 配置移除、旧规则恢复 | 通过 | `migrated_tcp_rule_can_rollback_from_proxy` |

### 3.4 自动化债务

| 编号 | 项目 | 当前结论 | 后续处理 |
|------|------|------|------|
| D-01 | `grpcs` Windows 本地稳定端到端自动化回放 | 已清偿 | `proxy_https_listener_tunnels_grpcs_prior_knowledge` 已落地并通过 |

## 4. 人工验收

### 4.1 环境准备

1. 以管理员身份启动应用，完成一次完整验收。
2. 以非管理员身份再次启动应用，仅验证权限与禁用态。
3. 准备以下测试资源：
   - 一个可写的临时目录，用于 Hosts 导入/导出
   - 两个本地 HTTP 服务，分别返回可区分内容
   - 一个本地 WebSocket 服务
   - 一个本地 HTTPS 服务
   - 如需验证 gRPC，再准备一个 h2c 或 TLS gRPC Demo 服务
4. 若验证本地 CA：
   - 确认系统可安装本地 CA 证书
   - 准备 `localhost` 或自定义测试域名映射到本机

### 4.2 Hosts 人工验收指南

#### A. 首次导入与分组初始化

1. 打开应用，进入 `Hosts` Tab。
2. 确认首次进入时自动生成 `default` 分组。
3. 打开 `default` 分组，检查表格中是否已出现从系统 `hosts` 文件导入的记录。
4. 验证记录字段：
   - `ip`
   - `domain`
   - `comment`
   - `enabled`

预期结果：

- 页面可正常显示
- `default` 分组存在
- 记录结构化显示，无明显丢行或错列

#### B. 分组新建、复制、删除

1. 新建一个分组，例如 `phase3-manual-a`。
2. 在其中新增 2-3 条 hosts 记录并保存。
3. 复制该分组，命名为 `phase3-manual-b`。
4. 打开复制后的分组，确认记录内容已完整复制。
5. 删除复制出的分组。

预期结果：

- 新建成功
- 复制后记录、顺序、启用状态一致
- 删除后列表刷新正常

#### C. 行编辑与保存

1. 在 `default` 或新建分组中：
   - 新增一条 IPv4 记录
   - 新增一条 IPv6 记录
   - 修改一条记录的备注
   - 切换一条记录的 `enabled`
2. 点击保存。
3. 刷新页面或切换分组后切回，确认数据持久化。

预期结果：

- 编辑可保存
- 非法 IP 不应允许保存
- 数据重载后仍与保存结果一致

#### D. 导入 / 导出

1. 选择一个分组，执行导出到临时目录。
2. 用文本编辑器打开导出文件，确认格式与系统 hosts 文件一致。
3. 将该文件重新导入为新分组。
4. 打开导入后的分组，确认记录与导出源一致。

预期结果：

- 导出文件格式正确
- 导入后记录数量、字段和值一致

#### E. 生效切换

1. 选择一个包含明显测试域名的分组。
2. 点击“设为生效”。
3. 检查系统 `hosts` 文件内容。
4. 再切换到另一分组并重复。

预期结果：

- 任意时刻只有一个分组显示为生效
- 系统 `hosts` 文件内容与当前生效分组一致

#### F. 非管理员行为

1. 关闭应用。
2. 以非管理员身份重新启动。
3. 进入 `Hosts` Tab。
4. 检查以下行为：
   - Tab 不隐藏
   - 页面出现管理员权限提示
   - 写系统文件相关操作被禁用

预期结果：

- 页面可见
- 只能查看，不能执行需要管理员权限的系统写入操作

### 4.3 Proxy 人工验收指南

#### A. HTTP 分流

1. 准备两个本地 HTTP 服务，返回不同文本，例如 `service-a` 与 `service-b`。
2. 在 `Proxy` 中新建一个 HTTP listener。
3. 新建两条 route：
   - `a.example.test`
   - `b.example.test`
4. 为两条 route 分别绑定不同 upstream。
5. 用浏览器或 curl 构造不同 `Host` 访问。

预期结果：

- `a.example.test` 命中 service-a
- `b.example.test` 命中 service-b

#### B. 默认路由

1. 在同一 listener 下再建一条默认路由。
2. 访问一个未配置的 host，例如 `fallback.example.test`。

预期结果：

- 请求命中默认路由
- 运行态中可见命中统计

#### C. 通配符匹配

1. 配置 `*.wild.example.test`。
2. 配置 `.root.example.test`。
3. 分别访问：
   - `api.wild.example.test`
   - `root.example.test`
   - `www.root.example.test`

预期结果：

- `*.wild.example.test` 仅匹配子域
- `.root.example.test` 同时匹配根域与子域

#### D. 路径改写

1. 将某条 route 配置为匹配 `/api`。
2. upstream 配置改写：
   - `from=/api`
   - `to=/`
3. 请求 `/api/users`。

预期结果：

- 上游收到 `/users`

#### E. HTTPS 与证书

1. 新建 HTTPS listener。
2. 验证手动证书上传：
   - 上传一组可用证书和私钥
   - 绑定到 listener
3. 验证本地 CA：
   - 新建 `local_ca` 证书
   - 绑定到 listener
   - 若已安装本地 CA，使用浏览器访问

预期结果：

- HTTPS listener 可启动
- 错误证书会被拦截
- 正常证书可建立连接

#### F. WebSocket / WSS

1. 准备本地 WebSocket 服务。
2. 配置 `ws` upstream，验证 Upgrade 与双向消息。
3. 准备本地 TLS WebSocket 服务。
4. 配置 `wss` upstream，重复验证。

预期结果：

- Upgrade 成功
- 可双向收发消息
- 运行态命中与错误统计正确

#### G. gRPC / gRPCS

1. 准备一个 h2c gRPC 服务。
2. 配置默认路由 + `grpc` upstream。
3. 发起一次标准请求。
4. 如需验证 `grpcs`：
   - 准备 TLS gRPC 服务
   - 配置 HTTPS listener + 默认路由 + `grpcs` upstream
   - 做一次人工连通性验证

预期结果：

- `grpc(h2c)` 主路径可用
- `grpcs(h2 over TLS)` 在人工场景中可验证连通性

说明：

- `grpcs` 的 Windows 本地稳定端到端自动化已补齐，可与人工连通性验证互相印证
- 人工联通性验证仍应执行

#### H. 默认路由异常与错误日志

1. 将默认路由指向一个不可用上游。
2. 发起请求。
3. 检查响应与运行态。
4. 查看错误日志。

预期结果：

- 请求被拒绝
- 出现错误日志
- route / upstream 错误计数增加

### 4.4 Legacy Rules 人工验收指南

#### A. Rules legacy 限制

1. 进入 `Rules` 页面。
2. 打开新建规则表单。
3. 检查可创建类型。

预期结果：

- 只能新增 `udp_fwd / socks5_proxy`
- `tcp_fwd / http_proxy` 不再允许新增

#### B. 旧规则迁移

1. 准备一条旧 `tcp_fwd` 规则。
2. 点击“迁移到 Proxy”。
3. 查看迁移预览。
4. 确认迁移。
5. 进入 `Proxy` 页面检查生成结果。

预期结果：

- 生成 listener / route / upstream
- 旧规则被禁用并标记迁移状态

#### C. `http_proxy` 草稿迁移

1. 准备一条旧 `http_proxy` 规则。
2. 执行迁移。
3. 检查生成结果。

预期结果：

- 生成 HTTP listener
- route / upstream 作为草稿存在
- 默认值符合设计
- 需要用户在 Proxy 中补全目标后再启用

#### D. 回滚

1. 对一条已迁移规则执行回滚。
2. 检查：
   - Proxy 配置是否删除
   - 旧规则是否恢复
   - 状态是否更新为已回滚

预期结果：

- 回滚成功
- 审计信息保留

## 5. 最终验收结果

### 5.1 自动化验收汇总

1. `Hosts`
   - `H-01 ~ H-05` 通过
   - `H-06` 记为人工验收项，当前未补稳定自动化
2. `Proxy`
   - `P-01 ~ P-06` 通过
   - `P-07` 中 `ws / wss / grpc(h2c) / grpcs` 全部通过
   - `P-08` 通过，依据未受信 TLS 上游回归验证 `502 + metrics`，错误日志走同分支
3. `Legacy Rules`
   - `L-01 / L-03 / L-04 / L-05` 通过
   - `L-02` 不符合预期：前端已收口，后端新增限制未收口

### 5.2 人工验收汇总

待执行。本清单已提供逐步操作指引，建议按 `Hosts -> Proxy -> Legacy Rules` 顺序完成。

### 5.3 阻塞项 / 债务项

1. `Hosts` 非管理员禁用态
   - 当前主要依赖人工验收
   - 需后续评估是否补 UI/E2E 自动化
2. `Legacy Rules` 新增类型后端限制
   - 当前仅前端入口收口
   - 若要求严格收口，需补后端校验并回归迁移路径
