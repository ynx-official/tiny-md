# GitHub 版本构建

推送以 `v` 开头的版本标签时，`.github/workflows/release.yml` 自动运行。
版本格式为 `vX.Y.Z`，也支持 `vX.Y.Z-beta.1` 等预发布标签。
普通分支推送不触发此工作流；不符合版本格式的 `v*` 标签会在构建前报错。

## 发布步骤

先将需要发布的代码提交并推送到 GitHub，再为该提交创建版本标签：

```sh
git push origin main
git tag -a v0.1.0 -m "Release v0.1.0"
git push origin v0.1.0
```

后续版本使用新的标签名称。工作流使用标签指向的代码，构建 macOS Apple Silicon
（arm64）和 Windows x64 两个版本。每个版本先执行格式检查、测试和 Clippy，
然后使用 release 配置构建。macOS 生成包含 `Tiny MD.app` 和 Applications 快捷方式的
DMG，打开后将应用拖入 Applications 即可安装。Windows 使用 Inno Setup 6 生成 EXE
安装程序，支持开始菜单快捷方式、可选桌面快捷方式和卸载；按当前用户安装，无需管理员权限。
Windows 使用 MSVC x64 工具链，并静态链接 C 运行库。

两个构建均成功后，自动创建对应的 GitHub Release，上传：

- `tiny-md-v0.1.0-macos-arm64.dmg`
- `tiny-md-v0.1.0-windows-x64-setup.exe`
- `SHA256SUMS`

预发布标签自动标为 GitHub prerelease。失败后可以在 Actions 中重新运行；
已有 Release 的附件会被同名新产物替换。工作流使用 GitHub 自带的 `GITHUB_TOKEN`，
无需额外配置发布密钥。Actions 构建产物额外保留 14 天。

应用包的版本取自标签中的 `X.Y.Z`，预发布后缀保留在安装包文件名和 Release 名称中。
标签不会修改源码中的 Cargo 包版本；发布前可自行同步 `Cargo.toml` 的工作区版本并更新
`Cargo.lock`。当前产物没有 Apple 开发者签名和公证，首次运行可能需要在系统设置中允许打开。

## 本地打包

```sh
sh scripts/bundle-macos.sh --release --version 0.1.0
sh scripts/package-macos.sh 0.1.0
```

输出应用包及 `target/release-assets/tiny-md-v0.1.0-macos-arm64.dmg`。
省略 `--version` 时使用 Cargo 包版本。
原有 `sh scripts/bundle-macos.sh --launch` 继续生成并启动 `target/Tiny MD.app` 开发包。

在配置好 Rust stable MSVC x64、Visual Studio C++ 工具及 Windows SDK 的 Windows
开发者 PowerShell 中运行（还需安装 Inno Setup 6）：

```powershell
./scripts/bundle-windows.ps1 -Version 0.1.0
```

只生成可直接运行的便携目录时使用 `./scripts/bundle-windows.ps1 -Version 0.1.0 -Portable`，
不需要 Inno Setup。产物为 `target/windows/Tiny MD/tiny-md.exe`，两种构建方式均在打包前
核验 x64 PE32+ 和 Windows GUI 子系统，防止双击应用时出现控制台。

输出为 `target/release-assets/tiny-md-v0.1.0-windows-x64-setup.exe`。需要 SDK 的 `fxc.exe`
编译 GPUI 发布版着色器；工作流会自动定位该编译器。Windows 应用的实际交互和输入法
仍需在 Windows 上验收。
