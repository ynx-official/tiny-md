# tiny-md

使用 Rust、GPUI、GPUI Component 和 Guise 构建的原生 Markdown 写作应用。
首个开发版本面向 macOS。

## 运行

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

应用图标位于 `assets/icons/`：白色圆角底、深色衬线 T 和浅灰文字行。
macOS 打包脚本会复制 `.icns` 并设置 `CFBundleIconFile`；Windows 构建脚本会将
多尺寸 `.ico` 嵌入 `tiny-md.exe`，需要 Windows SDK 的 `rc.exe` 或 GNU `windres`。
Windows 图标资源已配置，应用的平台适配仍待验证。
在 macOS 上运行 `sh scripts/build-icons.sh` 可从源图重新导出 PNG、ICNS 和 ICO。
macOS 启动时也会加载程序内嵌的 `.icns`，因此 `cargo run` 直接运行时会显示相同的 Dock 图标。

## 当前能力

- 简洁写作窗口、居中的文档标题与正文，默认隐藏工具栏和侧栏。
  文件、编辑、段落、格式、显示、主题、窗口和帮助操作使用 macOS 顶部原生菜单。
- 工具栏是正文底部的悬浮排版栏：段落 / 标题选择、粗体、斜体、行内代码、链接、
  图片链接、引用和列表；“更多”向上展开，提供源码模式、插入段落、复制 / 粘贴、
  清除样式、删除块与隐藏工具栏。图片按钮目前插入 Markdown 链接，尚无内嵌图片预览。
- 侧边栏有独立的“文档”和“大纲”视图。文档页显示当前文件夹内的 Markdown 文件、
  内容摘要和选中状态，可打开文件夹、刷新和切换文档；大纲只用于当前文档的标题导航。
  文档切换前检查未保存修改。当前为平面文件列表，不递归扫描子文件夹。
- 即时 Markdown 排版，正文所在行展开语法；标题标记保持隐藏，源码模式可直接编辑完整语法。
- 段落、标题、强调、代码、链接、引用、列表、任务列表。
- 代码块卡片、语言标签与复制按钮；进入代码块时展开围栏语法。
  高亮支持 Rust、Python、JavaScript、TypeScript、Go、C/C++、SQL、JSON、TOML 和 Markdown。
- Mermaid 流程图原生预览，点击展开源码；支持深浅主题、中文标签与语法错误提示。
- 表格网格直接编辑，支持自适应列宽、长文本换行、列对齐和单元格内 Markdown。
  Tab / Shift+Tab 切换单元格，Enter 跳到下一行，末行自动新增可撤销的空行。
  原生“段落 → 表格”菜单支持行列增删和列对齐，结构操作一次撤销即可恢复。
- 新建、独立窗口、打开、最近文件、保存、另存为、磁盘重新加载和 Finder 定位。
  关闭只关闭当前窗口；退出逐个检查各窗口的未保存修改，取消或保存失败会停止退出。
- 标题级别、引用、列表、代码块、表格、分割线、Front Matter、常用行内格式和行移动菜单。
- 查找替换（区分大小写、循环跳转、全部替换可一次撤销）。
- 大纲导航、字符统计、深浅主题、只读模式、专注模式、打字机模式和文字缩放。
- 源码模式共享同一个编辑器和撤销历史。
- GPUI 原生输入法接口，UTF-16 位置转换、组合输入和一次提交对应一次撤销。
- 普通方向键和删除按 Unicode 字素操作，保持组合 emoji 和附加符号完整。
- 后台文件读写，同目录临时文件原子替换，保存失败时保留编辑内容。
- 保留已打开文件的 UTF-8 BOM 和统一 CRLF；外部修改后的文件拒绝直接覆盖。

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
| `fixtures` | 中文与 Markdown 验收样例 |

锁定 **GPUI 0.2.2 / GPUI Component 0.5.0 / Guise 1.9.1**，共用同一个 GPUI。
Guise 的 MarkdownEditor 发布版未实现完整平台组合输入接口，因此适配层保留少量编辑器
视图和私有辅助文件。文本模型、Markdown 解析、布局映射、主题和高亮继续使用 Guise。
来源、许可证和升级方式见 [THIRD_PARTY.md](crates/editor-adapter/THIRD_PARTY.md)。

流程图使用 [mermaid-rs-renderer](https://github.com/1jehuang/mermaid-rs-renderer) 0.3.1
生成 SVG，由 GPUI 显示，无需 Node.js 或浏览器。它并非 Mermaid.js 的完整实现，
复杂语法和布局可能不同。流程图大小随窗口调整，预览高度最多 560 px。
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
外部修改保护、Markdown 大纲过滤、表格源码映射与单元格导航。

## 尚待实现

图片内嵌预览与拖放、标签页与文件树、导出 / 打印、自动保存与崩溃恢复、设置持久化、
数学公式、脚注和其他平台适配。最近文件和显示设置目前只保留在本次应用会话。

当前大纲只提取 ATX 标题；混合换行的文件在修改后保存时统一为 LF。
外部修改保护是保存前检查，尚不包含持续文件监听与自动合并。
