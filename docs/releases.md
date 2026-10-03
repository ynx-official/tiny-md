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
（arm64）和 Intel（x86_64）两个版本。每个版本先执行格式检查、测试和 Clippy，
然后使用 release 配置构建并打包 `Tiny MD.app`。

两个构建均成功后，自动创建对应的 GitHub Release，上传：

- `tiny-md-v0.1.0-macos-arm64.zip`
- `tiny-md-v0.1.0-macos-x86_64.zip`
- `SHA256SUMS`

预发布标签自动标为 GitHub prerelease。失败后可以在 Actions 中重新运行；
已有 Release 的附件会被同名新产物替换。工作流使用 GitHub 自带的 `GITHUB_TOKEN`，
无需额外配置发布密钥。Actions 构建产物额外保留 14 天。

应用包的版本取自标签中的 `X.Y.Z`，预发布后缀保留在 ZIP 文件名和 Release 名称中。
标签不会修改源码中的 Cargo 包版本；发布前可自行同步 `Cargo.toml` 的工作区版本并更新
`Cargo.lock`。当前产物没有 Apple 开发者签名和公证，首次运行可能需要在系统设置中允许打开。

## 本地打包

```sh
sh scripts/bundle-macos.sh --release --version 0.1.0
```

输出为 `target/release/Tiny MD.app`。省略 `--version` 时使用 Cargo 包版本。
原有 `sh scripts/bundle-macos.sh --launch` 继续生成并启动 `target/Tiny MD.app` 开发包。
