# Tiny MD

Tiny MD 是一款简洁的原生 Markdown 编辑器，适合日常笔记、技术文档和长文写作。
它把编辑与排版预览放在同一个界面中：输入 Markdown 即可看到排版效果，也可以随时切换到源码模式，
两个模式共享编辑内容与撤销历史。文档以本机 Markdown 文件保存，方便用其他编辑器继续处理。

应用使用 Rust、GPUI、GPUI Component 和 Guise 构建，提供 Windows x64 与 macOS 原生界面，
采用 [MIT 许可证](LICENSE) 开源。

## 主要特色

- **专注写作**：居中正文、按需展开的侧栏与排版工具栏，支持深浅主题、专注模式和打字机模式。
- **即时排版与源码编辑**：支持标题、列表、任务列表、引用、链接和代码块，切换模式时保留撤销历史。
- **表格与流程图**：直接编辑 Markdown 表格；预览 Mermaid 流程图，并在独立窗口中缩放、拖动查看。
- **文档管理与导航**：文档列表、目录树、大纲、查找替换和独立窗口，便于整理与浏览本机文档。
- **文件修改保护**：保存前检查磁盘变化，自动合并不重叠的修改；遇到冲突或保存失败时保留编辑内容。
- **应用内更新**：查看版本日志、检查并下载更新，安装前处理未保存文档。

图片内嵌预览、导出 / 打印、自动保存与崩溃恢复等能力尚未实现，完整范围见下方“尚待实现”。

## 下载与开始使用

从 [GitHub Releases](https://github.com/ynx-official/tiny-md/releases/latest) 下载对应平台的发行包：

| 平台 | 安装方式 |
|---|---|
| Windows x64 | 下载 setup EXE 安装，或解压便携 ZIP 后运行 `tiny-md.exe` |
| macOS Apple Silicon（arm64） | 下载 DMG，将 `Tiny MD.app` 拖入 Applications；也提供便携 ZIP |

启动后可新建文档开始写作，也可以从“文件”菜单打开已有 Markdown 文件。
从 v0.2.1 开始支持将 Markdown 文件拖入窗口，并为 Windows 安装增加右键入口和默认打开勾选项；
Windows 默认应用仍需在系统设置中确认，说明见 [拖放与文件集成](docs/05-operations/windows-shell-integration.md)。
使用 Ctrl+S（Windows）或 ⌘S（macOS）保存；侧栏和大纲可从“视图 / 显示”菜单打开。

[变更日志](CHANGELOG.md) · [版本总览](docs/06-delivery/versions/index.md) · [文档索引](docs/README.md)

## 从源码运行

需要 Rust stable 和 macOS Command Line Tools。
GPUI 已启用 `runtime_shaders`，运行时由 Metal 编译着色器，构建不需要额外安装完整 Xcode。

```sh
cargo run --locked -p tiny-md
cargo run --locked -p tiny-md -- /absolute/path/to/note.md
```

构建可双击的开发版应用：

```sh
sh scripts/bundle-macos.sh --launch
```

输出为 `target/Tiny MD.app`。这是本机开发包，尚未签名、公证或制作发行安装包。

Windows 需要 Rust stable MSVC、Visual Studio C++ x64 构建工具及 Windows SDK：

```powershell
./scripts/run-windows.ps1
./scripts/run-windows.ps1 -Document './fixtures/tables.md'
./scripts/check.ps1
./scripts/bundle-windows.ps1 -Portable
```

脚本自动加载编译器环境。Windows 操作使用 Ctrl 快捷键，例如 Ctrl+S 保存、Ctrl+H 替换、
Ctrl+Y 重做、F11 全屏。窗口内菜单、标题栏按钮与系统字体已按 `notion` 风格接入。
Windows 无文件启动时显示空白文档，布局按提供的 Typora 参考；底栏提供侧栏与源码按钮，
排版工具栏从“视图 → 工具栏”打开。可双击的优化版位于 `target/windows/Tiny MD/tiny-md.exe`。
顶部菜单提供 `文件(F)` 等 Alt 访问键提示，F10 打开文件菜单；下拉项目显示实际绑定的快捷键。
详见 [Windows 说明](docs/05-operations/windows.md) 和 [文档索引](docs/README.md)。

应用图标位于 `assets/icons/`：白色圆角底、深色衬线 T 和浅灰文字行。
macOS 打包脚本会复制 `.icns` 并设置 `CFBundleIconFile`；Windows 构建脚本会将
多尺寸 `.ico` 嵌入 `tiny-md.exe`，需要 Windows SDK 的 `rc.exe` 或 GNU `windres`。
Windows 图标资源、快捷键和文件操作已通过本机自动检查；原生交互仍待验收。
Windows 使用收紧留白的独立图标，macOS 使用去掉外框的满画布素材。
PowerShell 7.2+ 运行 `./scripts/build-icons.ps1` 可从母图重新导出 PNG、ICNS 和 ICO；
macOS 运行 `sh scripts/build-icons.sh` 可从已适配的平台素材导出 ICNS 和 ICO。
像素约束、消费路径与验证边界见 [应用图标说明](docs/02-design/application-icons.md)。
macOS 启动时也会加载程序内嵌的 `.icns`，因此 `cargo run` 直接运行时会显示相同的 Dock 图标。

## 当前能力

- 帮助菜单提供检查更新与版本变更日志；更新窗口支持下载进度、取消、重试和安装重启。
  自动检查可选择启动时、按间隔或关闭，偏好保存在本机；自动检查不自行下载或安装。
  安装前逐窗口处理未保存文档。开发构建只提供手动检查和下载。
  发布清单、GitHub API 和发布页面按顺序查询，下载内容必须通过 SHA-256 校验。
  详见 [在线更新](docs/03-architecture/online-updates.md) 和 [变更日志](CHANGELOG.md)。

- 简洁写作窗口、居中的正文，默认隐藏排版工具栏和侧栏；顶部文档文件名居中，Windows 应用标识位于左侧。
  文件、编辑、段落、格式、显示、主题、窗口和帮助操作使用 macOS 顶部原生菜单；
  Windows 使用窗口内菜单。
- 文档列表 / 文档树支持右键菜单：打开、新窗口打开、新建、搜索、文件简介、重命名、
  创建副本、删除到系统废纸篓 / 回收站、复制路径和文件管理器定位。
  文档树以当前目录名称为根节点，文件夹和文件带图标、按目录层级缩进，可展开 / 折叠；
  搜索可匹配文件路径和文档摘要。
  重命名保留编辑内容和撤销历史，并同步同一文件的其他窗口；新建和副本不覆盖同名文件。
- 工具栏是正文底部的悬浮排版栏：段落 / 标题选择、粗体、斜体、行内代码、链接、
  图片链接、引用和列表；“更多”向上展开，提供源码模式、插入段落、复制 / 粘贴、
  清除样式、删除块与隐藏工具栏。图片按钮目前插入 Markdown 链接，尚无内嵌图片预览。
- 侧边栏有文档列表、文档树和大纲视图。列表显示 Markdown 文件、内容摘要和选中状态；
  可拖动右侧分隔线连续调整宽度，切换视图和隐藏再打开时保留本窗口的调整结果；
  调宽时显示细虚线，内容超高时显示独立滚动条，支持滚轮、拖动滑块和点击轨道；
  文档树递归显示当前文件夹，底部显示目录名称，可打开文件夹、刷新和切换文档；
  大纲用于当前文档的标题导航。打开笔记保留已有文档和草稿，只复用没有待保存内容的空白未命名窗口。
- 即时 Markdown 排版，正文所在行展开语法；标题标记保持隐藏，源码模式可直接编辑完整语法。
- 段落、标题、强调、代码、链接、引用、列表、任务列表。
- 代码块卡片、语言标签与复制按钮；进入代码块时展开围栏语法。
  按围栏语言高亮 Rust、Python、JavaScript、TypeScript、Go、C/C++、Java、Shell、
  YAML、HTML/CSS、SQL、JSON、TOML 和 Markdown 等，支持常用语言别名。
  多行注释 / 字符串随代码块保留解析状态，深浅主题使用对应的语法颜色；
  未标注或未知语言保持纯文本。
- Mermaid 流程图原生预览，点击展开源码；支持深浅主题、中文标签与语法错误提示。
- 表格网格直接编辑，支持自适应列宽、长文本换行、列对齐和单元格内 Markdown。
  Tab / Shift+Tab 切换单元格，Enter 跳到下一行，末行自动新增可撤销的空行。
  原生“段落 → 表格”菜单支持行列增删和列对齐，结构操作一次撤销即可恢复。
- 新建、独立窗口、打开、最近文件、保存、另存为、磁盘重新加载和 Finder 定位。
  打开、最近文件及侧栏打开优先复用空白未命名窗口，已有笔记继续保留；文件对话框支持多选，其余文件独立打开。
  详见 [文档窗口](docs/02-design/document-windows.md)。
  关闭只关闭当前窗口；退出逐个检查各窗口的未保存修改，取消或保存失败会停止退出。
- 标题级别、引用、列表、代码块、表格、分割线、Front Matter、常用行内格式和行移动菜单。
- 查找替换（区分大小写、循环跳转、全部替换可一次撤销）。
- 大纲导航、字符统计、深浅主题、只读模式、专注模式、打字机模式和文字缩放。
- 源码模式共享同一个编辑器和撤销历史。
- GPUI 原生输入法接口，UTF-16 位置转换、组合输入和一次提交对应一次撤销。
- 普通方向键和删除按 Unicode 字素操作，保持组合 emoji 和附加符号完整。
- 后台文件读写，同目录临时文件原子替换，保存失败时保留编辑内容。
- 保留已打开文件的 UTF-8 BOM 和统一 CRLF；文件事件触发同步，5 秒字节核对兜底，保存前再核对并合并。
  未修改的文档自动更新，不重叠的本地修改自动合并；重叠冲突保留内容，提供另存本地副本或同步磁盘版本。
- 光标使用正文颜色并闪烁，引用块使用灰色竖线与灰色文字。

输入法实现有自动回归验证；真实系统中文候选、组合输入和候选框位置的验收状态见
[验证记录](docs/verification.md)。

| 快捷键 | 操作 |
|---|---|
| ⌘N / ⌘O | 新建 / 打开 |
| ⌘⇧N | 新建独立窗口 |
| ⌘S / ⌘⇧S | 保存 / 另存为 |
| ⌘/（兼容 ⌘⇧M） | 源码与即时排版切换 |
| ⌘⇧L | 显示 / 隐藏侧栏 |
| ⌃⌘1 / ⌃⌘2 | 大纲 / 文档列表 |
| ⌘F / ⌘⌥F | 查找 / 替换 |
| ⌘G / ⌘⇧G | 下一 / 上一匹配 |
| ⌘1…⌘6 / ⌘0 | 标题级别 / 普通段落 |
| F8 / F9 | 专注 / 打字机模式 |
| ⌘⇧= / ⌘⇧- / ⌘⇧0 | 放大 / 缩小 / 实际大小 |
| ⌘⇧T | 深浅主题切换 |
| ⌘B / ⌘I / ⌘K | 粗体 / 斜体 / 链接 |
| ⌘Z / ⌘⇧Z | 撤销 / 重做 |
| Tab / Shift+Tab（表格内） | 下一 / 上一单元格 |
| Enter（表格内） | 下一行同列，末行新增一行 |
| ⌘W / ⌘Q | 关闭窗口 / 退出应用（均检查未保存内容） |

## 架构与依赖

| 模块 | 职责 |
|---|---|
| `crates/app` | GPUI 窗口与原生菜单、Component 标题栏与查找输入框、侧栏、原生文件对话框 |
| `crates/editor-adapter` | Guise 编辑器适配、平台输入、源码模式 |
| `crates/document` | 无 UI 依赖的文件状态、保存、大纲提取 |
| `crates/updater` | 发布元数据、下载校验、更新偏好、平台安装助手 |
| `fixtures` | 中文与 Markdown 验收样例 |

锁定 **GPUI 0.2.2 / GPUI Component 0.5.0 / Guise 1.9.1**，共用同一个 GPUI。
Guise 的 MarkdownEditor 发布版未实现完整平台组合输入接口，因此适配层保留少量编辑器
视图和私有辅助文件。文本模型、Markdown 解析、布局映射和主题继续使用 Guise。
代码块采用 Syntect 5.3.0 的语言语法并映射到 Guise 主题色，TypeScript / TOML 等缺失语法
使用 Guise 高亮回退；未变化的代码块复用解析结果。验收样例见 `fixtures/code-highlighting.md`。
来源、许可证和升级方式见 [THIRD_PARTY.md](crates/editor-adapter/THIRD_PARTY.md)。

流程图使用 [mermaid-rs-renderer](https://github.com/1jehuang/mermaid-rs-renderer) 0.3.1
生成 SVG，由 GPUI 显示，无需 Node.js 或浏览器。它并非 Mermaid.js 的完整实现，
复杂语法和布局可能不同。流程图默认按可用宽度展示（自动放大最多 200%），
预览区域随图形实际高度展开，缩放或调整窗口宽度时同步更新高度，长图随正文滚动。
卡片提供放大 / 缩小、原始大小和适宽按钮，手动缩放支持 25%–300%，超宽图可横向滚动。
位图按实际显示尺寸与系统缩放生成，调整宽度或缩放时替换并释放旧资源。
打开文档即在后台渲染全部流程图，不再等待滚动到对应位置。
“查看”按钮打开最大化的独立窗口，默认适合整张图；支持滚轮缩放、左键拖动、
放大 / 缩小、100% 和适合窗口，Esc 关闭。查看内容是打开时的只读快照，不修改正文。
点击查看时一次性生成固定高分辨率整图，缩放、拖动和调整窗口大小只改变这张图的显示方式。
目标为 SVG 逻辑尺寸的 4 倍，最长边不超过 16384 像素，单张像素缓冲不超过 64 MiB；超大图按比例缩小。
超出位图分辨率的放大会逐渐变模糊。关闭按钮、Esc 或关闭快捷键会释放整图缓存与纹理。
渲染限制为 64 KB 源码和 200 个节点，超出限制保留源码并提示。
运行 `cargo run --locked -p tiny-md -- fixtures/code-and-diagrams.md` 可以查看验收样例。

表格使用 Markdown 原文位置映射，编辑和源码模式共享撤销历史；输入单个竖线时自动转义。
`fixtures/tables.md` 包含对齐、Unicode、缺失单元格和转义样例，
`fixtures/product-definition-v2.md` 保留用户提供的完整文档，含 24 个表格。
拖动列宽和跨单元格批量粘贴尚待实现。

编辑状态属于编辑器；文档层只保存磁盘基准与文件属性。
输入过程中订阅变化，不通过 `set_text()` 回填。

## 检查

```sh
sh scripts/check.sh
```

测试重点：组合输入提交与取消、选区替换、UTF-16/emoji 边界、撤销历史、文件保存失败、
外部修改合并与冲突保护、同步后的撤销与选区保留、Markdown 大纲过滤、表格源码映射与单元格导航。

检查脚本还运行发布工具测试和当前版本资料校验，需要 Python 3.11+。
首次使用在线更新需要手动安装一个包含该功能的新发行版；历史 `v0.1.0` 没有更新入口。
当前代码版本为 **0.2.1**，发布状态、改动与本次跳过测试的说明见 [版本详情](docs/06-delivery/versions/v0.2.1.md)。
实际验证与平台验收边界见 [在线更新验证](docs/04-quality/online-updates.md)。

## 尚待实现

图片内嵌预览与拖放、标签页、导出 / 打印、自动保存与崩溃恢复、设置持久化、
数学公式、脚注和 Linux 等其他平台适配。最近文件和显示设置目前只保留在本次应用会话。

当前大纲只提取 ATX 标题；混合换行的文件在修改后保存时统一为 LF。
外部修改采用父目录文件事件、定期字节核对与按行三方合并；同一行的重叠修改需要人工处理。
机制及验收规则见 [外部文件同步](docs/02-design/external-file-sync.md)。

## 许可证

Tiny MD 采用 **MIT 许可证**，完整条款见 [LICENSE](LICENSE)。

第三方依赖与改编代码保留各自的许可证和版权声明；Guise 编辑器适配说明见
[第三方代码说明](crates/editor-adapter/THIRD_PARTY.md)，原始许可见
[LICENSE.guise](crates/editor-adapter/LICENSE.guise)。
