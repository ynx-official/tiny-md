# 在线更新与发布验证

- 状态：Review
- 最后更新：2026-10-08
- 关联文档：[更新设计](../03-architecture/online-updates.md)、[发布流程](../releases.md)、[变更日志](../../CHANGELOG.md)

## 已运行证据

- 发布工具先运行失败测试，确认尚无发布资料实现；实现后 Python 回归通过，覆盖版本与锁文件不一致、
  日期错误、日志缺失、重复索引、不完整产物、公开版本覆盖拒绝、上传失败不公开草稿。
- 更新核心先运行版本比较与有效清单测试，确认失败；实现后回归通过。
- Rust 回归覆盖稳定通道与数字版本比较、平台与安装方式选择、清单校验、API 限流后的页面兜底、
  校验错误不降级、下载截断与错误哈希、取消解包、路径穿越、偏好持久化和 Windows 替换失败恢复。
- GPUI 回归确认取消未保存提示时保留原文与窗口，不启动安装；此前已确认的编辑窗口能够解除冻结。
- 真实更新源：`cargo run --locked -p tiny-md-updater --example check` 返回 `v0.1.0`，
  校验通过的附件为 macOS arm64 DMG（10,789,415 字节）与 Windows x64 setup（8,072,814 字节）。
- 真实取消下载：同一示例加 `--cancel-download`，首个数据块后取消成功。
- 真实完整下载：同一示例加 `--download`，Windows setup 大小与 SHA-256 校验通过；示例不执行安装，临时文件自动回收。
- Windows 独占临时目录清理已实际执行，覆盖 Rust canonicalize 返回的 `\\?\` 路径。
- Windows `bundle-windows.ps1 -Portable` 已生成 release 可执行文件和便携 ZIP，PE 检查确认 x64、PE32+、GUI 子系统。

## 最终检查命令

```powershell
./scripts/check.ps1
./scripts/bundle-windows.ps1 -Portable
./scripts/test-windows-update.ps1
```

最后一项在专属临时目录和 `target/update-smoke-*` 下运行真实更新助手，
用独立无窗口测试程序验证替换、版本探测、重启标记和清理。它不会替换用户现有安装，也不会打开写作窗口。
本轮最终结果：`check.ps1` 通过，包含 **116 项 Rust 测试、13 项 Python 测试**、
发布资料一致性、rustfmt 与 Clippy（`-D warnings`）；编辑器原有的 1 项文档示例继续处于 ignored。
Windows release 构建与便携包生成通过。真实更新助手的隔离测试通过：
旧文件被替换为新文件、版本探测成功、重启标记生成、专属临时目录回收。
最后一次测试目录为 `target/update-smoke-fc88a34d37624a598234a9668d14c2ac`，仅包含本轮测试程序与源码。
PowerShell 发布、验证与助手脚本均已通过语法解析检查；Windows PE 资源中的 ProductVersion 实测为 `0.1.0`。

## 尚未验收边界

- 本机未发现 Inno Setup 6 编译器，未构建或运行新的 Windows setup 安装包；CI 保留安装和卸载验证。
- macOS 的 ZIP 签名、应用 bundle 替换与原生重启需要 macOS runner / 实机，当前 Windows 环境未执行。
- 上述开发阶段未创建标签或运行云端发布；后续正式发布结果见下节。
- 自动回归不等于真实系统窗口、输入法、首次安装权限和安装版重启已经验收。

## v0.2.0 正式发布

用户于 2026-10-08 明确授权正式发版并要求跳过测试。
workspace 与四个锁文件包版本已统一为 `0.2.0`；版本详情、总览、CHANGELOG 日期与链接一致性校验通过。
`cargo metadata --locked --offline --no-deps` 与格式检查通过，本次不重新运行上述开发阶段测试。
CI 仅对 `v0.2.0` 跳过 Rust / Python、安装卸载和助手测试，保留格式 / Clippy、双平台构建和产物校验。
[发布流水线](https://github.com/ynx-official/tiny-md/actions/runs/37755327430) 第 2 次运行成功，
正式 Release 已于 2026-10-08 17:42:56（Asia/Shanghai）公开，7 个附件齐全。
已核对更新清单、远端附件名称 / 大小 / digest、校验和及发布正文，并实际下载核验 3 个元数据文件。
完整证据以 [v0.2.0 版本详情](../06-delivery/versions/v0.2.0.md) 为准；这不代表安装或原生交互已验收。

## 未发布的 Markdown 日志与构建优化

用户在 `v0.2.0` 标签推送后要求支持 Markdown 日志渲染和最高打包优化，并明确让本次发布继续。
这两项只修改工作区，记录在 `Unreleased`，不改版本号或已推送标签，也不进入该标签的构建产物。
当前版本与远端日志使用 GPUI Component TextView；浅色和深色分别保存渲染状态，以更新代码高亮颜色。

2026-10-08 本机 Windows 最终源码的 `cargo fmt --all -- --check`、
`cargo clippy --locked --workspace --all-targets -- -D warnings` 和最高优化 release 构建均通过。
构建采用 `opt-level = 3`、fat LTO、单代码生成单元和静态 MSVC 运行库，
`verify-windows-exe.ps1` 确认产物为 Windows x64 GUI 程序。完整编译耗时 14 分 13 秒，
随后针对最终源码再次构建通过；此耗时只作为本次构建记录，不用于性能对比。
按用户要求未运行测试，未验收原生日志界面、macOS 上的新配置或运行性能 / 包体变化。

## v0.2.1 正式发布

用户于 2026-10-08 再次要求开始发版并跳过测试，以上日志与构建优化及文件 / 窗口 / 图标改动纳入 `v0.2.1`。
workspace 与四个锁文件包版本统一为 `0.2.1`，本次不运行 Rust / Python、安装卸载或更新助手测试。
工作流只对本次新标签跳过测试，保留格式 / Clippy、实际双平台构建和发布完整性校验。
[发布流水线](https://github.com/ynx-official/tiny-md/actions/runs/37768093173) 首次运行成功，
正式 Release 于 2026-10-08 19:30:35（Asia/Shanghai）公开，`latest` 指向 `v0.2.1`。
已核对 7 个附件、清单的 4 个产物、6 项校验和及发布正文，并实际下载核验 3 个元数据文件。
历史 `v0.2.0` 附件的 ID、大小与 digest 均保持不变。
完整证据以 [v0.2.1 版本详情](../06-delivery/versions/v0.2.1.md) 为准；安装与原生交互仍未验收。
