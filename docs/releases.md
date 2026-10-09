# 版本资料、构建与发布

- 状态：Review
- 最后更新：2026-10-09
- 关联文档：[更新设计](03-architecture/online-updates.md)、[验证记录](04-quality/online-updates.md)、[版本总览](06-delivery/versions/index.md)、[CHANGELOG](../CHANGELOG.md)

## 版本与日志来源

根 `Cargo.toml` 的 `workspace.package.version` 是唯一代码版本来源，四个 workspace 包继承该版本。
应用关于信息、更新比较和 `--version` 均读取编译版本。发布标签必须精确等于 `v` 加该版本，
例如 `v0.2.0` 或 `v0.2.0-beta.1`；锁文件中的四个包版本也必须一致。

版本详细正文只维护在 `docs/06-delivery/versions/vX.Y.Z.md`，包含版本概述、用户变化、兼容性、
升级方法、已知问题、真实验证结果与变更依据。`CHANGELOG.md` 只保留精简摘要和详情链接，
`docs/06-delivery/versions/index.md` 维护当前代码版本与历史索引。

`scripts/release_notes.py` 校验以上资料，提取用户正文生成 `release-notes.md`，
排除验证记录、变更依据和站内导航。该文件同时用于 GitHub Release 正文和在线更新日志。
应用的离线当前版本日志在构建时从同一版本详情嵌入，使用相同的正文过滤规则。

## 准备一个新版本

1. 从实际差异和验证结果整理 `CHANGELOG.md` 的 `Unreleased`；确定新版本号。
2. 更新 workspace 版本与 `Cargo.lock`，建立对应 `vX.Y.Z.md`，同步总览当前版本、日期、条目和 CHANGELOG 详情链接。
3. 日期属于发布准备资料，详情标记 Review，索引明确“待发布”；不要声称未执行的验证已通过。
4. 运行相关检查与打包，把实际结果写回版本详情，然后运行 `python -B scripts/release_notes.py --check-current` 和 `./scripts/check.ps1`。
5. 用户确认正式发布后，提交所需代码和资料、创建与代码版本匹配的新标签并推送。只有推送 `v*` 标签才触发 Release 工作流。
6. 发布成功后核对附件和清单，再将总览标记为“已发布”。不复用已经公开的标签，不覆盖公开附件。

`v0.1.0` 已于 2026-10-03 公开，新增更新机制使用新版本 `v0.2.0` 发行。
用户分别明确要求 `v0.2.0`、`v0.2.1` 和本次 `v0.2.2` 跳过测试，各标签中的工作流仅对本次版本
跳过 Rust / Python、安装卸载与助手测试；
版本资料、格式 / Clippy、构建、包格式 / 签名与附件校验保留，后续版本默认恢复测试。

## 自动流水线

`.github/workflows/release.yml` 先在独立任务中运行发布工具测试、版本与文档一致性校验。
通过后并行构建 macOS Apple Silicon（arm64）与 Windows x64；每个平台先检查格式、测试和 Clippy。
macOS 对应用进行 ad-hoc 签名，生成 DMG 和便携 ZIP；Windows 使用静态 MSVC 运行库，
生成按当前用户安装的 Inno Setup 6 EXE 与便携 ZIP，核验 PE32+ / GUI 子系统。
Windows CI 默认另外验证安装、卸载，以及隔离安装版 / 便携更新助手；v0.2.2 按要求跳过这些测试。

两个平台均成功后，发布任务从同一标签检出脚本，生成正文、清单及校验和。
以 `v0.2.0` 为例，Release 附件为：

- `tiny-md-v0.2.0-macos-arm64.dmg`
- `tiny-md-v0.2.0-macos-arm64-portable.zip`
- `tiny-md-v0.2.0-windows-x64-setup.exe`
- `tiny-md-v0.2.0-windows-x64-portable.zip`
- `release-notes.md`
- `update-manifest.json`
- `SHA256SUMS`

清单记录四个平台 / 安装方式产物的精确文件名、URL、大小、SHA-256 和日志地址。
`SHA256SUMS` 同时覆盖四个产物、正文和清单。
先创建草稿并上传所有附件，核对远端附件名称与大小完全一致后才公开 Release；
任一失败均保留草稿。预发布标签使用 GitHub prerelease，不进入稳定版自动更新通道。

工作流使用 GitHub 自带的 `GITHUB_TOKEN`，不引入更新服务器或额外发布密钥。
Actions 产物保留 14 天。公开版本重跑只允许校验和相同的幂等结果，
不会重新上传；重新构建或日志变化导致校验和不同，必须升级版本。

## 本地构建与探测

### v0.2.1 构建优化

`v0.2.1` 将 release 配置显式设为 `opt-level = 3`（最高数值等级）、
`lto = "fat"`（全量链接时优化）、`codegen-units = 1` 和 `strip = true`。
Windows 与 macOS 打包脚本均使用这一 Cargo profile；当前 `v0.2.0` 仍按标签中的 thin LTO 构建。
全量 LTO 会增加编译时间和构建内存，实际性能或包体变化需要测量，不预设收益。
保持默认目标 CPU 和 panic 策略，避免改变发布包的 CPU 兼容范围与错误处理行为。

### 构建命令

Windows 需要 Rust stable MSVC x64、Visual Studio C++ 工具、Windows SDK 和 Python 3.11+。
脚本自动加载编译器环境；只有 setup 打包需要 Inno Setup 6。

```powershell
./scripts/bundle-windows.ps1 -Portable
./scripts/test-windows-update.ps1
./scripts/bundle-windows.ps1
```

输出目录分别为 `target/windows/Tiny MD/` 和 `target/release-assets/`。
传入 `-Version` 时必须与 Cargo 完全一致，不能仅改变文件名。
助手隔离测试要求 PowerShell 7，编译无窗口测试程序，仅修改 `target/update-smoke-*` 和本次专属临时目录。
它不会打开原生写作窗口或更新用户的现有安装。

macOS 需要 Rust stable、Command Line Tools 和 Python 3.11+：

```sh
sh scripts/check.sh
sh scripts/bundle-macos.sh --release
sh scripts/package-macos.sh "$(cargo pkgid -p tiny-md | sed 's/.*[@#]//')"
```

应用元数据按平台要求使用核心 `X.Y.Z`，二进制版本、安装包文件名和日志保留完整预发布后缀。
`sh scripts/bundle-macos.sh --launch` 继续生成开发包；开发构建不执行安装更新。

仅验证更新源和下载，不执行安装：

```sh
cargo run --locked -p tiny-md-updater --example check
cargo run --locked -p tiny-md-updater --example check -- --download
cargo run --locked -p tiny-md-updater --example check -- --cancel-download
```

后两条验证现有 Windows setup 附件的完整下载或中途取消，临时数据随后自动清理。

## 用户升级与验收边界

历史 `v0.1.0` 没有在线更新功能，用户需要先手动安装一次包含更新机制的新发行版。
此后安装版继续使用 EXE 更新；便携 Windows 和 macOS `.app` 使用各自 ZIP，
用户确认“安装并重启”后才替换应用，所有写作窗口先通过未保存检查。

新版本助手会再次核验归档和程序版本，等待旧进程退出，替换失败恢复旧程序并保留日志。
其他实例使用同一可执行路径时停止 Windows 安装，安装器禁止强制关闭应用。
macOS 仅更新可写位置的 `.app`，不能直接替换 DMG 内应用。

当前没有 Apple 开发者签名与公证；ad-hoc 签名不等于发布者身份签名。
SHA-256 校验用于下载完整性，信任边界是正式 GitHub 仓库与 TLS。
具体自动验证和未完成的原生平台验收见 [在线更新验证](04-quality/online-updates.md)。
