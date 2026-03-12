# WSL Bridge Windows 管理员运行方案

## 背景

`wsl-bridge` 的核心能力涉及：

- Windows 防火墙规则创建与删除
- 端口监听与转发规则应用
- WSL / Hyper-V 拓扑探测
- 后续通过 MCP 对外暴露系统侧控制能力

结合当前产品定位与 `docs/wsl-bridge-design.md` 中“整个应用统一管理员权限运行”的约束，入口程序应在启动时直接申请 Windows 管理员权限，而不是在运行后按功能局部提权。

## 选型结论

采用方案 A：**发布版入口 EXE 强制 `requireAdministrator`**。

实现方式：

- 为最终发布的 `wsl-bridge.exe` 嵌入自定义 Windows manifest。
- 在 manifest 中设置：
  - `requestedExecutionLevel level="requireAdministrator"`
  - `uiAccess="false"`
- 通过 `src-tauri/app/build.rs` 在 Tauri 构建阶段注入该 manifest。

## 方案效果

用户启动应用时：

1. Windows 先弹出 UAC。
2. 用户授权后，`wsl-bridge.exe` 主进程以管理员权限启动。
3. 由该主进程直接创建的子进程，默认继承管理员上下文运行。

这适用于当前项目里的：

- `netsh`
- `powershell.exe` / `pwsh.exe`
- `wsl.exe`
- 后续由 Rust 主进程直接拉起的 helper 或系统命令

## 边界说明

该方案保证的是“**本应用入口进程及其直接派生进程**”运行在管理员权限中，不保证所有外部关联进程都会自动被提升。

需要额外注意的边界：

- 如果后续增加独立 `helper.exe` / `updater.exe`，建议它们也带明确 manifest。
- 如果某些流程不是由主进程直接 `CreateProcess`，而是通过 Explorer、任务计划或其他外部链路启动，则是否提权取决于各自启动链。

## 为什么不选运行时自举提权

不采用“普通进程启动后再 `runas` 重启自己”的原因：

- 启动链更复杂，单实例和参数透传处理更麻烦。
- 开发与发布行为更容易分叉。
- 当前产品没有“普通权限模式”需求，强制管理员更符合现状。

## 实施清单

1. 在 `src-tauri/app/` 下新增 Windows manifest 文件，声明 `requireAdministrator`。
2. 修改 `src-tauri/app/build.rs`，通过 `tauri-build` 注入自定义 manifest。
3. 保留默认 manifest 中的 `Microsoft.Windows.Common-Controls` 依赖，避免 Windows 对话框等能力出现兼容性回退。
4. 维持现有所有系统命令都从 Rust 主进程发起，不引入前端侧提权逻辑。
5. 校验发布态编译链路，确认构建脚本和 manifest 语法正确。
6. 在开发文档中记录该权限模型，明确开发调试建议：
   - `pnpm tauri dev` 建议从管理员终端启动。
   - 运行中的子进程统一从主进程创建。

## 当前实施范围

本次只实现入口 EXE 的强制管理员权限运行，不额外改造：

- 自动更新链路
- 独立 sidecar / helper 二进制
- 前端权限提示 UI
- 非 Windows 平台的权限模型
