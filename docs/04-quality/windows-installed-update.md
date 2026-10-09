# Windows 安装版在线更新修复

- 状态：Review
- 最后更新：2026-10-09
- 关联文档：[更新设计](../03-architecture/online-updates.md)、[更新验证](online-updates.md)、[发布流程](../releases.md)
- 发布状态：纳入 v0.2.2，已授权正式发布并跳过本次测试；不修改已公开版本和附件。

## 现象与已确认原因

用户报告 Windows 点击“安装并重启”后重新打开原版，没有安装页面。
本机两次遗留更新目录的 `install.log` 都记录安装器退出码 3，计划为 `kind=setup`，
目标版本为 `v0.2.1`，当前路径为 `\\?\C:\...\Programs\Tiny MD\tiny-md.exe`。
安装版识别和产物选择正确，失败发生在安装器交接。

旧助手将 `canonicalize` 得到的扩展路径直接写入 `/DIR`。
本机 Inno Setup 6.7.3 的独立无注册表安装 fixture 使用相同参数时拒绝目录中的 `?`，
退出非零且不写入目标；换成普通绝对路径即退出 0 并成功写入。
初次启用目录页面的 fixture 退出码为 1；再次使用与安装包相同的隐藏目录页面设置，
复现退出码 3 且可执行文件未安装，日志同样明确拒绝 `?`。
第二份证据是 `target/installed-update-smoke-203eed35ce3446feaa15183e2cb5d633/baseline-no-compat.log`。
证据在 `target/update-installer-regression-ac5c79f7e28544e89007d94479440ffc/`。
旧代码还使用 `/VERYSILENT` 和 `/SUPPRESSMSGBOXES`，隐藏进度与错误；
失败恢复后重新启动旧程序，仅写入日志，因此用户容易误以为更新完成。

## 修复边界

- 助手只把磁盘与 UNC 扩展路径转换为普通绝对路径；拒绝设备路径和相对路径，完整保留 UTF-16 文件名。
- 路径在就绪交接前检查，安装参数用独立 `OsString` 传递，避免空格、中文、单引号、`$()` 与 `&` 被解释为命令。
- `/SILENT` 显示自动安装进度；`/NORESTART`、`/CURRENTUSER` 和 `/NOCLOSEAPPLICATIONS` 保留。
- 安装器写 `installer.log`；退出码错误携带日志位置。交接后失败由原生 Windows 对话框提示，交接前仍由更新窗口提示。
- 其他实例检查同时处理磁盘与 UNC 扩展前缀，不关闭其他实例。
- 新安装包初始化时规范化旧助手的 `/DIR`；否则旧客户端依旧复制旧助手，首次升级仍会失败。
- 替换失败恢复可执行文件和已有版本 / 欢迎文件，保留诊断目录；不宣称还原所有 Inno 注册状态。

## 回归与实际执行

新增路径与进度 / 日志参数的两项 Rust 回归，先运行确认失败，再实现修复。
`cargo test --locked -p tiny-md-updater` 的 14 项测试通过，覆盖来源 / 版本 / 平台选择、
下载完整性 / 解包、路径规范化、备份恢复和偏好保存。
更新模块 Clippy 的所有目标检查通过。

隔离安装版脚本使用真实 Inno 安装器和真实更新助手，只安装独立测试程序，
关闭卸载器 / 卸载注册表项，没有注册表或快捷方式段，不接触用户安装。
已使用无界面测试驱动执行下列情况：

| 情况 | 实际结果 |
|---|---|
| 不带兼容代码的安装器 + 新助手，计划含 `\\?\` | 安装成功、核对新版、启动新版、清理暂存 |
| 兼容安装器 + 新助手 | 安装成功、启动新版、清理暂存 |
| 本机已安装 v0.2.1 的旧助手 + 兼容安装器 | 成功完成升级，无需先更换旧助手 |
| 安装写入后返回非零退出码 42 | 恢复旧可执行文件、版本与欢迎文件，保留安装日志 |
| 安装后的可执行文件版本不符 | 拒绝作为成功，恢复旧文件，保留日志 |
| 同一安装位置另有实例运行 | 安装器未执行，其他实例继续运行，旧文件保持原样 |

初次六项完整执行证据在 `target/installed-update-smoke-d11e681f81db48e9ad81d42f6f8d9656/`。
最终脚本增加旧参数直接安装检查，以及失败恢复后实际重新启动旧版本的断言；七项再次通过，
证据在 `target/installed-update-smoke-203eed35ce3446feaa15183e2cb5d633/`。
发布流水线增加安装版成功检查，并使用无界面驱动执行失败恢复检查；便携更新检查保留。

其余最终执行证据：

- 原失败目录内的真实 v0.2.1 setup，长度与计划相同、SHA-256 与计划一致，排除下载损坏。
- 两项 GPUI 更新退出回归通过：取消保存保留文档、不启动安装，撤销安装恢复已确认窗口。
- `cargo fmt --all -- --check`、更新模块及应用所有目标 Clippy、17 项发布工具回归和当前发布资料检查通过。
- `cargo build --locked --release -p tiny-md --target x86_64-pc-windows-msvc` 通过，最高优化构建用时 4 分 14 秒。
- 用上述真实 release 程序复制出的助手运行隔离安装版：旧参数兼容、原始安装器 / 兼容安装器升级均通过。
  证据在 `target/installed-update-smoke-9b9ec941ec9a4766830ac57215391e37/`。
- 同一 release 程序的便携更新助手通过替换、版本探测、新版启动与清理；
  证据在 `target/update-smoke-e69da6a61366468ba30a6b698a917702/`。
- 实际主安装脚本与兼容 include 编译成功；x64 GUI 和 9 个内嵌图标帧验证通过。
  本地包是 `target/update-fix-preview/tiny-md-unreleased-windows-x64-setup.exe`，
  基于当前工作区，内部版本仍为 `0.2.1`，供手动验证，不能冒充新的正式发布附件。
  SHA-256：`9c7e5a86a13cb5139f5b345882564eaa894d3662d46c4923b57491c5f69fe230`。

复现命令（`Compiler` 可指定本机便携 Inno 编译器）：

```powershell
. ./scripts/setup-windows.ps1
cargo test --locked -p tiny-md-updater
cargo build --locked -p tiny-md-updater --example update_helper
./scripts/test-windows-installed-update.ps1 -Application target/debug/examples/update_helper.exe -Compiler target/tools/inno-setup-6.7.3/ISCC.exe -IncludeFailures -LegacyHelper "$env:LOCALAPPDATA/Programs/Tiny MD/tiny-md.exe"
```

## 验收边界

隔离测试核验安装 / 版本 / 重启 / 文件恢复，不代表正式用户安装目录的升级已经验收。
UNC 前缀转换已做单元回归，未在真实网络共享运行安装。
安装进度与原生错误弹窗的可见性尚未人工验收；macOS 逻辑未修改且本机未验收。
修复纳入新的 `v0.2.2`，正式发布与跳过测试的记录见 [版本详情](../06-delivery/versions/v0.2.2.md)。
上述开发阶段测试证据保留，不作为升级版本后的测试重跑证据。

## 官方依据

- [Inno 安装参数](https://jrsoftware.org/ishelp/topic_setupcmdline.htm)：`/SILENT` 显示进度，`/VERYSILENT` 隐藏进度，`/LOG` 保存诊断。
- [Inno 退出码](https://jrsoftware.org/ishelp/topic_setupexitcodes.htm)：非零退出码表示未完成安装，不能作为成功重启。
- [InitializeWizard 事件](https://jrsoftware.org/ishelp/topic_scriptevents.htm)：初始化后、安装前调整目录输入。
