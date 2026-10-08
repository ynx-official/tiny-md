# Windows 开发与适配

- 状态：Review
- 最后更新：2026-10-08
- 关联文档：[文档索引](../README.md)、[验证记录](../verification.md)、[版本构建](../releases.md)

## 平台与环境

目标环境为 Windows 10/11 x64，Rust stable MSVC 工具链、Visual Studio 2022 C++ x64
构建工具及 Windows SDK。本轮自动测试实际运行在 Windows 11 x64、Rust 1.96.0。
Windows 10 兼容性尚待实际运行验收。

`scripts/setup-windows.ps1` 自动加载 C++ x64 开发环境，并定位 SDK 的 `fxc.exe`。
可通过已有的 `GPUI_FXC_PATH` 指定着色器编译器；有效的显式配置会被保留。
脚本不会安装软件，不修改系统 PATH。

## 运行与检查

在项目根目录的 PowerShell 中执行：

```powershell
./scripts/run-windows.ps1
./scripts/run-windows.ps1 -Document './fixtures/tables.md'
./scripts/run-windows.ps1 -Release
./scripts/check.ps1
./scripts/bundle-windows.ps1 -Portable
```

传入的文档路径会在切换工作目录前解析，包含空格或中文的路径保持为一个参数。
`check.ps1` 依次执行格式检查、工作区测试和 Clippy；任一步失败均终止。

编译后的开发程序为 `target/debug/tiny-md.exe`，Windows GUI 子系统避免双击时出现控制台。
发行安装程序仍由 `scripts/bundle-windows.ps1` 构建，需要额外安装 Inno Setup 6；
本轮环境尚未安装 Inno Setup 6，未生成新版本安装程序；已构建便携优化版并更新当前用户已有安装目录。
`-Portable` 无需 Inno Setup，生成可双击的静态 C 运行库优化版
`target/windows/Tiny MD/tiny-md.exe`。打包前校验 x64 PE32+ 和 GUI 子系统，拒绝带控制台的程序。

本机已有 Tiny MD 安装时，可用 `./scripts/update-windows-local.ps1` 将该优化版更新到
当前用户的 `Programs/Tiny MD`。脚本先核验并暂存新程序，再将旧 EXE 保留为唯一名称的
`.exe.bak`，替换程序并核对 SHA-256；原有快捷方式继续指向 `tiny-md.exe`。
已打开的旧程序继续使用旧映像，关闭后再次打开才使用新版本。若 Windows 拒绝移动正在运行的
程序，脚本保留现有程序并提示关闭后重试。

## 快捷键

| 快捷键 | 操作 |
|---|---|
| Ctrl+N / Ctrl+O | 新建 / 打开 |
| Ctrl+Shift+N | 独立窗口 |
| Ctrl+S / Ctrl+Shift+S | 保存 / 另存为 |
| Ctrl+W / Ctrl+Q | 关闭窗口 / 退出应用 |
| Ctrl+F / Ctrl+H | 查找 / 替换 |
| Ctrl+G / Ctrl+Shift+G | 下一个 / 上一个匹配 |
| Ctrl+/ / Ctrl+Shift+M | 源码模式 |
| Ctrl+Shift+L | 显示 / 隐藏侧栏 |
| Ctrl+Alt+1 / Ctrl+Alt+2 | 大纲 / 文档列表 |
| Ctrl+1…6 / Ctrl+0 | 标题级别 / 普通段落 |
| Ctrl+B / Ctrl+I / Ctrl+K | 粗体 / 斜体 / 链接 |
| Ctrl+Z / Ctrl+Y / Ctrl+Shift+Z | 撤销 / 重做 / 重做 |
| Ctrl+Shift+T | 深浅主题 |
| F8 / F9 / F11 | 专注 / 打字机 / 全屏 |

macOS 继续使用原有 Command 快捷键。Windows 的替换入口使用 Ctrl+H；
macOS 的隐藏应用快捷键仅在 macOS 注册。

文件名校验拒绝 Windows 保留设备名（包括带扩展名的 `CON`、`NUL`、`COM1` 等），
避免新建和重命名误操作设备路径。原有 BOM、CRLF、同名文件保护和原子保存行为
已通过 Windows 自动测试。

## 窗口内菜单与字体

已按确认的 `notion` 风格及随后提供的 Typora 截图接入独立菜单行，提供文件、编辑、段落、格式、视图、主题和帮助操作。
菜单使用各文档窗口自己的状态；主题、只读、源码、工具栏、侧栏及
最近文件变化时刷新菜单。Windows 不显示 macOS 的服务和隐藏应用操作，退出位于文件菜单。

标题栏提供最小化、最大化 / 还原和关闭按钮，长文档名截断以保留按钮区域。
应用图标和标题位于左侧，底栏常驻侧栏与源码模式按钮；排版工具栏从“视图 → 工具栏”打开。
顶部标题栏和菜单行总高度为 52 px；正文外层左右留白为 12 px、顶部为 16 px，宽屏下最大文档宽度为 1080 px。
底栏源码按钮可启用 / 退出源代码模式，提示和选中状态跟随当前模式。
无文件路径启动时显示空白文档，快速入门通过帮助菜单打开。
菜单标题中的 F / E / P / O / V / T / H 分别对应 Alt+F / Alt+E / Alt+P / Alt+O / Alt+V / Alt+T / Alt+H。
F10 也能打开文件菜单。下拉项目显示已有的实际快捷键，菜单方向键和 Escape 使用原有弹出菜单行为。
Windows 标题栏显示文档名和未保存的 `•` 标记；标题间距已收紧，文档原有空行仍然保留。
正文和控件使用 Segoe UI，行内代码与代码块使用 Consolas。样式范围与适配原因见
[UI 风格](../02-design/ui-style.md)。

光标采用正文颜色的细线并闪烁；引用块使用灰色竖线和文字。
已打开文件会后台同步其他应用的修改，非重叠编辑自动合并并保留撤销记录。
同一处修改冲突时，底栏可选择保留本地副本或同步磁盘版本；规则见
[外部文件同步](../02-design/external-file-sync.md)。

## 尚待验收

此前原生自动化曾被用户 Escape 停止；本轮已确认菜单可见、灰色引用、光标显隐、
原生打开文件、外部同步与合并保存、冲突取消 / 显式同步以及撤销重做，详见验证记录。
各菜单的 Alt 访问键、左右切换、主题及全部菜单动作尚未完整交互验收。
本机已安装程序与已验证的便携优化版哈希一致；需关闭旧窗口后重新从原快捷方式启动。
此前隐藏启动检查也未取到主窗口句柄或标题，详见验证记录。
真实系统中文候选、候选框位置、原生文件对话框、资源管理器定位、回收站操作及
多窗口保存提示仍需要原生交互验收，自动逻辑测试不能代替这些验收。
