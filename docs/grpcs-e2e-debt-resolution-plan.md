# `grpcs` Windows 本地稳定 E2E 自动化债务解决方案

## 1. 文档信息

- 项目：`wsl-bridge`
- 关联债务：
  - `docs/phase3-proxy-hosts-design.md` -> `12.5 Proxy 自动化债务`
  - `docs/phase3-task-breakdown.md` -> `12.2 三期债务项`
  - `docs/phase3-acceptance-checklist.md` -> `D-01`
- 更新时间：`2026-05-16`
- 目标：清偿三期唯一明确登记的 Proxy 技术债务
- 当前状态：`已实现并通过验证`

## 2. 债务定义

当前明确登记的技术债务是：

1. `grpcs` 的 **Windows 本地稳定端到端自动化回放**

当前状态：

1. `grpcs(h2 over TLS)` 运行时链路已接入。
2. 配置约束已具备稳定自动化：
   - 必须绑定 `HTTPS Listener`
   - 必须走默认路由
   - 不支持路径改写
3. `HTTPS Listener`、证书装载、本地 CA、上游 TLS 信任链已有稳定自动化。
4. 唯一未稳定的是完整链路：
   - `TLS client -> HTTPS Listener -> grpcs TLS upstream`
5. 历史症状：
   - Windows 本地回放偶发 `10053`
   - 表现为连接被本机/对端异常中止，导致 E2E 测试 flaky

结论：

这不是“功能未实现”问题，而是“完整双 TLS 隧道在 Windows 本地测试环境下的收尾与回放稳定性问题”。

## 3. 根因分析

### 3.1 现状

当前 `grpcs` 运行时入口位于：

- [forwarder.rs](C:/Users/yangholmes.liang/work/wsl-bridge/src-tauri/crates/core/src/forwarder.rs)
  - `handle_grpcs_prior_knowledge_tunnel()`

当前链路做了这些事情：

1. 入站 HTTPS listener 完成 TLS 终止
2. 识别 `HTTP2_PRIOR_KNOWLEDGE_PREFACE`
3. 选择默认 route 下最新启用的 `grpcs` upstream
4. 建立上游 TCP
5. 建立上游 TLS client 连接
6. 写入 HTTP/2 preface
7. 后续走双向字节流透传

### 3.2 高风险点

从当前实现和历史现象看，风险主要在以下几类：

1. `grpcs` 当前复用了偏 `wss` 风格的 TLS 双工 relay 逻辑
   - 两者都属于双向流
   - 但关闭语义不同
   - `wss` 风格的主动 `close_notify` / 半关闭时机不一定适合 `h2 over TLS`
2. relay 中存在较强的关闭干预
   - 一侧 EOF 后主动关闭另一侧
   - 主动发送 `send_close_notify()`
   - 在 Windows 回环环境中容易放大竞态
3. 自动化夹具对“何时退出”控制不够强
   - client / upstream / listener 线程的退出次序可能有窗口
   - 某一侧还在 flush 时另一侧先 shutdown，容易触发 `10053`
4. 目标验证过于靠近“真实业务 gRPC”
   - 如果直接拉高到完整 gRPC 行为，调试面会变得过大

### 3.3 根因判断

最可能的根因不是“代理转发能力错误”，而是：

1. `grpcs` 的 tunnel relay 语义与 `wss` relay 语义混用
2. TLS close / socket shutdown 顺序在 Windows 本地环境下不稳定
3. 自动化夹具没有把生命周期控制到足够可重复

## 4. 解决目标

### 4.1 本次清债目标

本次不是去追求“完整真实 gRPC 应用级回归”，而是先达成：

1. 提供稳定的 `grpcs` Windows 本地 E2E 自动化
2. 证明以下事实：
   - HTTPS listener 可以接住入站 TLS
   - `grpcs` upstream TLS 握手成功
   - HTTP/2 preface 被完整转发
   - 后续双向 payload 可以透传
   - route / upstream metrics 正常
   - 不产生误报错误日志

### 4.2 不在本次目标内的内容

以下内容不作为这次清债的前置要求：

1. 引入完整第三方 gRPC server/client 生态做业务级联调
2. 覆盖 unary / streaming / metadata / trailers 全语义
3. 在首次提交里同时完成真实 gRPC 业务回归

说明：

先解决“稳定 E2E 自动化存在”这个问题，再决定是否扩展到更真实的 gRPC 行为验证。

## 5. 总体方案

推荐分两条线推进：

1. 运行时语义收敛
2. 测试基建收敛

原则：

1. `grpcs` 不再继续借用偏 `wss` 语义的 relay
2. 为 `grpcs` 增加单独的 raw TLS tunnel relay
3. E2E 测试只验证“最小可控闭环”，不一上来追求完整 gRPC 协议语义

## 6. 运行时改造方案

### 6.1 新增专用 relay

建议在 `forwarder.rs` 中为 `grpcs` 下沉专用 relay，语义定义为：

1. 入站：
   - `StreamOwned<ServerConnection, TcpStream>`
2. 出站：
   - `StreamOwned<ClientConnection, TcpStream>`
3. 转发策略：
   - 完全按 opaque bytes 双向透传
   - 不做 WebSocket Upgrade 语义假设
   - 不做额外协议级处理
4. 关闭策略：
   - 一侧 EOF 后仅做温和收尾
   - 避免过早向另一侧发送 `close_notify`
   - 将常见 Windows 收尾错误识别为 graceful close

建议函数：

1. `relay_grpcs_tunnel_streams(...)`
2. 或者更通用地命名为 `relay_tls_tunnel_streams(...)`

但如果做通用函数，必须保留清晰的关闭策略参数，不建议再隐式复用 `wss` 策略。

### 6.2 `handle_grpcs_prior_knowledge_tunnel()` 的调整

保留当前流程框架，但替换最后的 relay 调用：

当前：

1. 建上游 TLS
2. 写 preface
3. 调用当前 `relay_https_listener_wss_streams(...)`

调整后：

1. 建上游 TLS
2. 写 preface
3. 调用 `relay_grpcs_tunnel_streams(...)`

### 6.3 关闭语义建议

建议收尾策略如下：

1. 入站 EOF：
   - 标记 inbound closed
   - 尽量只 flush outbound
   - 不立即发 `close_notify`
2. 出站 EOF：
   - 标记 outbound closed
   - 尽量只 flush inbound
   - 不立即打断另一侧剩余读取
3. 循环退出条件：
   - 双侧都 closed
   - 或遇到非 graceful error
4. graceful error 判定：
   - `UnexpectedEof`
   - `ConnectionAborted`
   - `BrokenPipe`
   - `ConnectionReset`
   - 必要时补充对 Windows 特定错误码的兼容识别

核心原则：

避免 relay 主动替 HTTP/2 over TLS 做过早的关闭决策。

## 7. 测试基建设计

### 7.1 测试目标

做一条 **最小闭环** 的 `grpcs` 本地 E2E，而不是完整业务级 gRPC：

1. 本地 TLS client 连接 HTTPS listener
2. client 写入：
   - `HTTP2_PRIOR_KNOWLEDGE_PREFACE`
   - 一段自定义 payload，例如 `ping`
3. upstream TLS server 验证收到：
   - 完整 preface
   - 自定义 payload
4. upstream 回写：
   - 一段自定义 payload，例如 `pong`
5. client 读回 `pong`

如果这个闭环稳定通过，就已经足以证明：

1. 入站 TLS termination OK
2. 出站 TLS upstream OK
3. 双向 h2 opaque tunnel OK

### 7.2 为什么不直接做真实 gRPC

因为本轮目标是“清理稳定自动化债务”，不是“把测试夹具复杂度做满”。

真实 gRPC 会引入更多不必要变量：

1. HTTP/2 frame 编解码
2. gRPC metadata / trailers
3. 业务 handler 生命周期
4. 第三方库初始化与线程模型

这些会拖慢定位效率。

### 7.3 测试夹具要求

夹具必须满足：

1. 本地自建证书
   - 继续复用现有测试证书工具
2. upstream server 先 bind 再 accept
   - 避免端口竞争窗口
3. listener 进入 `Running` 后再发 client 请求
4. client / upstream 线程退出顺序可控
5. 不依赖固定 `sleep` 作为主同步机制
6. 如涉及共享本地 CA 目录，继续沿用互斥锁

### 7.4 测试断言

建议断言：

1. client 成功建立 TLS
2. upstream 成功读取完整 `HTTP2_PRIOR_KNOWLEDGE_PREFACE`
3. upstream 成功读取额外 payload
4. client 成功读回响应 payload
5. route runtime:
   - `hit_count = 1`
   - `error_count = 0`
6. upstream runtime:
   - `hit_count = 1`
   - `error_count = 0`
7. listener runtime:
   - `state = Running`
   - `last_error = None`

可选断言：

1. 无新增错误日志

## 8. 分阶段落地计划

### 阶段 A：运行时语义拆分

目标：

1. 将 `grpcs` 从现有 TLS 双工 relay 中拆出
2. 引入独立的 tunnel relay

任务：

1. 分析 `relay_https_listener_wss_streams()` 与 `grpcs` 的差异
2. 新增 `relay_grpcs_tunnel_streams()`
3. 替换 `handle_grpcs_prior_knowledge_tunnel()` 中的 relay 调用
4. 保持现有 `ws / wss` 回归不受影响

完成标准：

1. 现有全部 core 测试仍通过

### 阶段 B：最小 E2E 夹具

目标：

1. 新增一条稳定的 `grpcs` 本地 E2E

建议测试名：

1. `proxy_https_listener_tunnels_grpcs_prior_knowledge()`

任务：

1. 自建 inbound HTTPS listener 证书
2. 自建 upstream TLS server
3. 自建 TLS client 发起请求
4. 验证 preface + payload 双向透传

完成标准：

1. 测试单独运行稳定通过
2. 连续多次运行稳定通过

### 阶段 C：稳定性收尾

目标：

1. 消除 flaky 行为

任务：

1. 补充对 Windows 特定收尾错误的 graceful 识别
2. 去掉不必要的固定 sleep
3. 必要时增加 barrier / channel 同步
4. 复核 close / flush 顺序

完成标准：

1. 全量 `cargo test` 稳定
2. 不再出现历史 `10053` 波动

### 阶段 D：文档收口

任务：

1. 更新 `docs/phase3-proxy-hosts-design.md`
2. 更新 `docs/phase3-task-breakdown.md`
3. 更新 `docs/phase3-acceptance-checklist.md`
4. 更新 `docs/开发日志.md`

完成标准：

1. 从“三期债务项”中移除 `D-01`
2. 将 `P-07` 从“债务”更新为“通过”

## 9. 模块改造点

### 9.1 后端 Rust

- [forwarder.rs](C:/Users/yangholmes.liang/work/wsl-bridge/src-tauri/crates/core/src/forwarder.rs)
  - 新增 `grpcs` 专用 tunnel relay
  - 调整 `handle_grpcs_prior_knowledge_tunnel()`
  - 必要时补充 Windows graceful close 判定

- [engine.rs](C:/Users/yangholmes.liang/work/wsl-bridge/src-tauri/crates/core/src/engine.rs)
  - 新增 `grpcs` 稳定 E2E 回归测试
  - 如有必要补充测试工具函数

### 9.2 文档

- [docs/phase3-proxy-hosts-design.md](C:/Users/yangholmes.liang/work/wsl-bridge/docs/phase3-proxy-hosts-design.md)
- [docs/phase3-task-breakdown.md](C:/Users/yangholmes.liang/work/wsl-bridge/docs/phase3-task-breakdown.md)
- [docs/phase3-acceptance-checklist.md](C:/Users/yangholmes.liang/work/wsl-bridge/docs/phase3-acceptance-checklist.md)
- [docs/开发日志.md](C:/Users/yangholmes.liang/work/wsl-bridge/docs/开发日志.md)

## 10. 不建议的方案

### 10.1 在现有 flaky 用例上继续堆 sleep / retry

不建议原因：

1. 只能压制现象
2. 不能修正 relay 语义问题
3. 会留下“偶尔绿”的测试

### 10.2 直接引入完整第三方 gRPC 回归

不建议原因：

1. 调试维度过大
2. 无法快速定位是 TLS、relay 还是 gRPC 语义问题
3. 成本与当前清债目标不匹配

### 10.3 为了通过测试而放宽 TLS 校验

不建议原因：

1. 会削弱当前已建立的上游信任链约束
2. 会让测试通过失去实际价值

## 11. 验收标准

本债务完成后，应满足以下标准：

1. 新增稳定自动化测试：
   - `proxy_https_listener_tunnels_grpcs_prior_knowledge()` 或等价测试
2. 该测试在 Windows 本地环境下稳定通过
3. 全量验证通过：
   - `cargo test --manifest-path src-tauri/crates/core/Cargo.toml`
   - `cargo test --manifest-path src-tauri/app/Cargo.toml`
   - `pnpm typecheck`
4. 文档中移除以下债务表述：
   - `grpcs` Windows 本地稳定端到端自动化回放

## 12. 推荐执行顺序

建议按以下顺序实施：

1. 先改 relay 语义
2. 再搭最小 E2E 夹具
3. 再做稳定性消抖
4. 最后更新文档并收口债务

## 13. 最终结论

这条债务的正确解法不是“继续硬补 flaky 测试”，而是：

1. 把 `grpcs` 从当前偏 `wss` 语义的 relay 中拆出来
2. 给 `grpcs` 配一条专用的 raw TLS tunnel relay
3. 用最小可控的双 TLS + HTTP/2 preface E2E 测试夹具来证明链路

这样做的收益是：

1. 运行时语义更清晰
2. 自动化更稳定
3. 后续若要升级到真实 gRPC 回归，也有明确基础
