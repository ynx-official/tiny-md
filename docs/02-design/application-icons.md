# 应用图标与平台适配

- 状态：Review
- 最后更新：2026-10-08
- 关联文档：[项目说明](../../README.md)、[文档索引](../README.md)、[UI 风格](ui-style.md)

## 问题与范围

用户要求修复图标大小，并参考本地会话「生成跨平台软件图标」。本轮仅调整 Tiny MD 图标素材、导出脚本和验证入口，保留深色衬线 T、浅灰文字行及白色背景，不改应用布局、版本或发布状态。

已测量的原素材：1024px 窗口 PNG 在 Alpha ≥ 128 时主体只有 798 × 762px；Windows ICO 的 16px 帧只有 12 × 12px、32px 帧只有 24 × 24px。原母图几乎不可见的零散像素还覆盖了大部分画布，因此不能按所有 Alpha > 0 像素简单裁切。

参考会话中用户确认使用 macOS 26+。Apple 说明系统会调整旧图标外形并叠加材质，建议减少源图自带的阴影和斜面。本轮据此采用独立满画布 macOS 素材；Tiny MD 的真实 Dock 表现仍需实机验证。[Apple 图标说明](https://developer.apple.com/videos/play/wwdc2025/220/)

## 权威素材与消费路径

| 文件 | 用途 |
| --- | --- |
| `assets/icons/tiny-md-source.png` | 保留的原始透明母图，Windows 导出的输入 |
| `assets/icons/tiny-md-macos-source.png` | macOS 独立不透明母图，背景延伸至四边与四角 |
| `assets/icons/tiny-md.png` | 1024px Windows 适配图；窗口左侧 16px 应用标识使用它 |
| `assets/icons/tiny-md.ico` | Windows EXE 的资源 ID 1；包含 16/20/24/32/40/48/64/128/256px |
| `assets/icons/tiny-md.icns` | macOS `.app` 与直接运行程序内嵌的 Dock 图标 |

Windows GPUI 的原生窗口/任务栏标识读取 EXE 资源；窗口内自绘应用标识读取 PNG。两条路径需要同时更新并重新构建。`crates/app/build.rs` 已跟踪 ICO 的变更，本轮保留该机制。

## 导出规则

Windows 从原母图的 Alpha ≥ 8 范围计算阴影边界，排除几乎不可见的噪点；再按 Alpha ≥ 128 的实心主体居中，避免底部阴影使图标偏移。围绕主体对称保留阴影空间，等比例缩放，最长边占输出画布 98%。每个 ICO 尺寸独立采样，保持透明四角。原始母图未被覆盖。

macOS 使用内置 `imagegen` 编辑原图，提示约束为：保留中央深色衬线 T 和两侧两条短行、三条长行，移除最外层圆角底板的斜面、描边、外部阴影与透明留白，将柔和白色纸面延伸到整个正方形；不加字、不加框。最终母图保存于项目内，不依赖用户级生成目录。

macOS 的 16/32px 普通尺寸继续使用 `ic04`/`ic05` 原生 ARGB 四平面 RLE；Retina 与较大尺寸使用 PNG 槽位，最大 1024px。原 ICNS 小尺寸编码有效，本轮并非修复该编码缺陷。

Windows 导出无需额外图像库，使用 PowerShell 7.2+ 和系统绘图 API：

```powershell
./scripts/build-icons.ps1
./scripts/build-icons.ps1 -OutputDirectory target/icon-preview
```

macOS 使用 `sips`、`iconutil` 和 Python 标准库：

```sh
sh scripts/build-icons.sh
```

macOS 脚本从独立母图生成 ICNS，从已提交的 Windows 适配 PNG 生成 ICO，不会再把 macOS PNG 覆盖成 Windows 窗口图标。若修改原始 Windows 母图，需要先运行 PowerShell 导出脚本更新适配 PNG。

## 检查与验收边界

```powershell
python -B -m unittest discover -s scripts -p 'test_app_icons.py'
./scripts/verify-windows-exe.ps1 -Path target/debug/tiny-md.exe
```

像素测试无需第三方 Python 依赖，检查 Windows 各尺寸的主体占比、居中、透明四角、DPI 帧列表，以及 macOS 的不透明背景、浅色四角和 ARGB/PNG 颜色一致性。已在旧素材上复现 Windows 占比和 macOS 透明内框失败；新素材四项测试通过，脚本总计 17 项测试通过。

新 Windows ICO 的 Alpha ≥ 128 可见范围：16px 为 16 × 15px，32px 为 30 × 29px；1024px 与全部 ICO 帧均满足两个方向至少占画布 87.5% 的回归下限。

EXE 检查只映射 PE 资源，不执行程序，逐帧比对资源 ID 1 的目录和全部 PNG 字节。替换素材后，旧 debug EXE 已因尺寸帧不一致被拒绝。该核对已接入现有 Windows 打包和本地更新入口。macOS 的 `scripts/check.sh` 额外调用系统 `iconutil` 解码已提交的 ICNS。

本机为 Windows，未执行 macOS 原生解码或 Dock 交互。满画布资源针对参考会话中的 macOS 26+ 场景；macOS 12–15 的外形兼容性未经实机验收，可能保留方形外形。已安装的旧 EXE、旧快捷方式与运行中的旧进程不会因为工作区素材变化而自行更新。

`cargo build --locked -p tiny-md` 已通过，新的 debug EXE 的九个图标帧与项目 ICO 逐字节一致。
Windows 导出再次运行至独立目录后，PNG、ICO、ICNS 的 SHA-256 与已安装素材一致；
PNG 测试解码器也与独立 Pillow 解码结果逐像素一致。PowerShell 和 shell 脚本语法检查、
格式检查及发布资料一致性检查通过。

首次运行 `scripts/check.ps1` 时，工作区中另一项正在实施的侧栏拖拽功能出现三项 Rust 测试失败
（`sidebar_resize_tests`）；本轮未修改侧栏代码或测试。该功能更新后重新运行
`cargo test --locked --workspace` 与 `cargo clippy --locked --workspace --all-targets -- -D warnings`，
全量测试与 Clippy 均已通过。原有一项编辑器文档测试仍按项目配置忽略，未改变测试跳过规则。

`scripts/bundle-windows.ps1 -Portable` 已完成优化版构建，九个内嵌 ICO 帧全部与素材一致，
EXE 也包含当前窗口 PNG 的完整字节。便携 ZIP 中的 EXE 与已核对的构建一致；
输出位于 `target/windows/Tiny MD/tiny-md.exe`。

已通过现有 `scripts/update-windows-local.ps1` 同步本机安装副本，保留旧 EXE 备份；
安装副本的哈希与便携版一致，九个图标帧再次核对通过。已向 Windows Shell 通知该应用、
桌面和开始菜单快捷方式的图标刷新。没有停止旧进程；保存内容并关闭旧窗口后，
从原快捷方式重新启动即可加载新图标。该本机构建不改变正式版本或已公开附件。
