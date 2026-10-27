# Markdown 中的 HTML 兼容

- 状态：Review
- 最后更新：2026-10-27
- 关联文档：[项目说明](../../README.md)、[图片预览](inline-images.md)、[Markdown 交互](markdown-interactions.md)、[输入光标](input-caret-synchronization.md)

## 已确认范围与实现方式

用户于 2026-10-27 确认实现常见 HTML 混排、`details` / `summary` 和 HTML 表格，Windows 与 macOS 共用逻辑，保存不改写 HTML 源码，并保持中文组合输入和撤销。

同日用户提供 `<p align="center"><a><img></a>…</p>` 徽章示例，指出源码分行导致徽章逐张竖排。此兼容性修复增加图片组的行内排版；保留原有编辑与保存边界。

同日用户要求补齐 `<h3>` 等 HTML 标题；`h1`–`h6` 映射至对应 Markdown 标题的排版规则。

同日用户提供 Typora 截图及 Neutrino Proxy 完整样例，要求独立 `<br/>` 与 `<!-- hello -->` 等无正文内容的语法仍可见、可定位。此要求替代此前预览隐藏所有注释的规则：原生编辑预览显示轻量功能标记，保留原有源码、点击编辑与撤销边界。

沿用 Rust、GPUI 与 Guise。HTML 是编辑器的展示层：先由现有 Markdown AST 识别文本、HTML 与代码边界，再用 `html5gum 0.8.4` 读取标签、已解码属性及源码字节范围。完整实体使用现有 `markdown` 库解码，并单独记录范围。排版结果保留源码到可见文本的映射，不建立可写 DOM，也不把排版结果序列化回文档。

`html.rs` 负责解析与源文映射，`editor_html.rs` 负责原生表格与折叠控件，`shaped_text.rs` 统一硬换行的绘制、点击与光标坐标。它们由 Windows / macOS 共用。图片复用现有加载器及资源限制；平台输入接口、编辑模型、撤销历史和磁盘保存继续使用原有实现。

## 支持的语法

| 类别 | 支持内容 | 展示规则 |
|---|---|---|
| 标题 | `h1`–`h6` | 独占源码行的标题使用对应 Markdown 标题字号、字重和留白；支持大小写标签、多行内容、对齐及嵌套文字样式 |
| 换行 | `br`、`br/`、`br /`，大小写均可 | 独占源码行的标签显示淡灰色等宽原文，占一个普通行高；行内及表格内继续实际换行 |
| 注释 | 完整 `<!-- … -->`，支持多行 | 柔和金色等宽原文，内部 Markdown、标签与实体不解释；可点击、选择、复制及编辑 |
| 图片 | `img` 的 `src`、`alt`、`width`、`height` | 本地相对路径、原生绝对路径、`file:`、HTTP/HTTPS；尺寸按可用宽度约束 |
| 基础文字 | `b` / `strong`、`i` / `em`、`s` / `del`、`u`、`mark`、`code`、`kbd`、`sub`、`sup` | 字重、斜体、删除线、下划线、标记、等宽与上下标 |
| 实体 | `&amp;`、`&lt;`、`&gt;`、`&nbsp;`、命名实体及十进制 / 十六进制数字实体 | 支持完整的 `&…;`；未知或未完成的实体保留原文 |
| 容器 | `p`、`div`、`span` | 保留文字及嵌套样式；同行的段落边界换行 |
| 对齐 | `align`、`style="text-align:…"` | `left` / `center` / `right`；图片可继承容器对齐 |
| 有限样式 | `color`、`background-color`、`font-weight`、`font-style`、`text-decoration` | 支持三位 / 六位十六进制颜色和常见颜色名；加粗、斜体、下划线、删除线 |
| 链接 | `a href` | 沿用正文链接交互；接受 HTTP/HTTPS、mailto 和无协议的相对目标 |
| 折叠 | `details`、`summary`、`open` | 默认收起，`open` 默认展开，支持嵌套；没有自身 summary 时显示“详细信息” |
| 表格 | `table`、`caption`、`thead`、`tbody`、`tfoot`、`tr`、`th`、`td` | 原生网格、表头、标题、文字样式、链接、换行、图片、`rowspan` / `colspan` |

颜色名支持 red、green、blue、black、white、gray / grey、orange、purple、yellow、teal、navy。
图片宽高支持正数像素值及内联 `style` 中的 `width` / `height` 像素值；仅设置一边时保留比例，设置两边时采用指定比例。
HTML 表格图片显示在对应单元格内。只包含图片、图片链接、可选 `span`、注释和空白的 `p` / `div` 按图片组横向排版，忽略源码缩进和换行，继承容器对齐，宽度不足时自动折行；`br` 显式换行。各徽章间距为 8 px，加载 / 失败显示占位，滚动条拖动期间冻结组的布局尺寸。图片链接沿用 Ctrl 点击（macOS 为 ⌘ 点击、只读为普通点击）打开规则。

普通点击图片组会展开其源码供编辑，离开后恢复横排。组内注释随图片组源码隐藏，组外注释作为提示文字显示；折叠正文内的注释与独立换行标记也随正文收起。含文字或未知标签的混合容器继续保留原有逐行展示，不把正文吞入图片组；表格单元格暂不合并图片组。不实现通用浏览器的文字与图片行内环绕。

功能标记沿用项目原生字体和主题：换行标记使用淡灰色，注释使用当前主题的黄色文字色阶（浅色 7、深色 4），不添加边框、背景或额外卡片。点击对应行后进入原有源码编辑样式；退出后恢复提示色。显示出的标签及注释就是原始字符，不是额外插入文档的文字。

图片链接向应用传递原始目标，HTTP/HTTPS 由浏览器打开；`LICENSE` 等其他目标沿用现有应用行为，在状态栏显示链接地址。

## 编辑与保存

- 行内 HTML 在阅读时隐藏标签；编辑相应行时显示原文。源码模式始终显示原文。
- 点击 HTML 表格单元格后展开该表格源码进行编辑，离开后恢复预览。HTML 表格不使用 Markdown 管道表格的增删行列命令。
- 点击折叠箭头只改变当前视图，不添加或删除源码里的 `open` 属性，不产生正文修改或撤销项。光标或选区进入折叠正文时自动展开以保持可编辑。
- 光标、选择、输入法候选位置与撤销使用原始字符 / UTF-16 范围；实体映射不返回 UTF-8 字符内部字节。
- 不调整标签大小写、引号、属性顺序、缩进或实体拼写。保留原有文档编码标记与换行格式；回归通过真实应用保存流程逐字节检查 BOM + CRLF 文件。
- 块级 HTML 内的 Markdown 标记按原文展示；希望在 details 中书写 Markdown 时，像示例一样在 summary 与正文之间留空行。行内 HTML 可与 Markdown 标记混排。

## 兼容边界

这是一组常见 HTML 的原生映射，不提供浏览器 CSS 布局。未支持标签、未闭合标签及编辑中的不完整语法保留为可编辑源码；不自动补齐或格式化。标签配对深度上限为 128。

标题沿用编辑器逐源码行布局。建议各标题与正文分别换行书写；同一行的连续标题会分行显示，但混合不同级别标题或正文时保留文字样式，使用正文行尺寸，避免把整行正文放大。表格单元格仍使用表格字号。此项只扩展 HTML 标题展示，大纲继续识别 Markdown 标题。

表格要求显式、正确配对的行与单元格，不自动补齐浏览器允许省略的结束标签。嵌套、无效或在同一源码行开始的多个表格完整回退到源码；多个表格可分别换行书写。单个表格最多 128 列、2048 个输入行，跨行 / 跨列值约束在 1–128；超出列数 / 行数的表格整体回退。各列等宽，保留源码中的单元格空白，不实现完整 CSS 表格算法。

不执行脚本、事件属性、iframe、object、外部样式表、任意 CSS 或 `javascript:` 链接。代码块、行内代码和 Front Matter 中的 HTML 不渲染；图片的网络、文件、解码限制沿用[图片预览](inline-images.md)。SVG 内部资源限制不变。

## 验证与验收边界

实现前的回归实际复现了标签直接显示、HTML 图片缺失、折叠正文未隐藏、HTML 表格无预览；后续边界回归还复现了 HTML 块内 Markdown 被解释、隐藏代码块留白、超宽表格部分显示、实体紧邻结束标签的范围错误。对应修复均保留回归测试。

自动验证覆盖：

- 嵌套样式、中文与 emoji、完整实体、未知 / 未闭合 / 转义 / 代码内标签及安全链接。
- HTML 六级标题与 Markdown 标题尺寸一致性、多行与缩进内容、嵌套样式、块间换行、标题级别变化的缓存刷新，以及标题中的中文组合输入 / 撤销。
- 折叠开关、单行 / 多行摘要、嵌套折叠、无摘要默认标题，以及折叠正文中的代码块。
- 表格合并单元格、标题保留、点击编辑、同长度内容修改后的缓存失效、继承样式和原文标记。
- HTML 图片的相对路径、尺寸、顺序、对齐及单元格内显示，并复用既有图片加载回归。
- HTML 内容中的中文预编辑、提交、候选位置、切换源码模式与撤销；实际保存 / 另存后的字节一致性。

Windows 实际执行 `./scripts/check.ps1`：258 项工作区测试、17 项发布工具测试、版本资料检查、格式检查及 `cargo clippy --locked --workspace --all-targets -- -D warnings` 全部通过。工作区通过数量为应用 74、文档 16、编辑器 153、更新器 15；原有 2 项手工流程图性能测试及 1 项 doctest 保持忽略。原始检查日志位于本地构建目录 `target/html-workspace-check.log`。

徽章反馈修复先通过 GPUI 测试复现缺少行内图片组；修复后新增 3 项回归覆盖跨源码行的居中横排、窄窗口换行、显式 `br`、不同徽章链接、点击编辑、中文组合输入 / 撤销、源码切换、注释隐藏和混合内容回退。重新执行标准检查：261 项工作区测试（编辑器 156 项）、17 项发布工具测试、格式、版本资料及 Clippy 全部通过，忽略项不变。日志：`target/html-badges-workspace-check.log`。徽章布局证据来自 GPUI TestPlatform，不作为 macOS 原生验收证据。

标题扩展新增 5 项回归，其中标题相邻边界测试实际复现了 `甲乙` 未换行的问题，修复后通过；同级 Markdown 尺寸、多行缩进、嵌套样式、缓存刷新、中文组合输入 / 撤销与源码切换均通过。应用保存测试加入大小写混合、引号与实体的 HTML 标题，确认 BOM + CRLF 字节保留。重新执行 `./scripts/check.ps1`：266 项工作区测试（应用 74、文档 16、编辑器 161、更新器 15）、17 项发布工具测试、版本资料、格式与 Clippy 全部通过，忽略项不变。日志：`target/html-headings-workspace-check.log`；输入证据仍为 TestPlatform，未新增系统输入法或 macOS 原生验收。

功能标记调整新增 4 项回归。修复前实际复现独立 `<br/>` 被转换为空换行、注释被隐藏；修复后验证逐字符源文映射、一个普通行高、点击定位、中文组合输入 / 撤销、选择复制、源码切换、代码与转义边界、表格内换行及折叠隐藏。用户整段样例的自动回归保留全部 URL，只在图片加载边界预置尺寸，保证测试不依赖网络；验证 3 个注释、2 个独立换行标记、标题居中及徽章横排。保存测试加入注释和大小写换行标签，继续逐字节验证 BOM + CRLF。`./scripts/check.ps1` 通过 270 项工作区测试（应用 74、文档 16、编辑器 165、更新器 15）、17 项发布工具测试、版本资料、格式与 Clippy；忽略项不变。日志：`target/html-functional-marks-workspace-check.log`。

Windows 最新开发构建已打开完整样例，原生截图确认金色注释、淡灰色换行标记、实际本地 Logo、居中标题及四个远程徽章显示。随后用户按 Esc 停止 Computer Use，界面自动操作已停止；此次不宣称原生点击交互、系统输入法或 macOS 已验收。

检查期间 D 盘空间耗尽导致编译 / 链接中断。可重建的增量编译缓存已暂存至 `C:/Users/Administrator/AppData/Local/Temp/tiny-md-html-build-cache-20261027/`，保留恢复能力；释放空间后完整检查重跑通过。此操作未移动项目源码或文档。

Windows 开发构建已启动示例；原生界面已检查文字样式、上下标、换行、对齐和折叠展开。用户按 Esc 停止 Computer Use 后停止界面自动操作，因此没有完成原生系统输入法、完整表格交互及全部界面验收。自动输入测试使用 GPUI TestPlatform，不能替代系统输入法验收。macOS 本次没有编译或运行证据，共用代码不等于已完成 macOS 原生验收。

手动检查示例：[HTML 混排检查](../../fixtures/html-compatibility.md)。
徽章反馈示例：[HTML 徽章混排](../../fixtures/html-badges.md)，使用用户提供的图片地址；实际远程图片是否成功仍取决于网络及上游响应。
标题反馈示例：[HTML 标题](../../fixtures/html-headings.md)，包含六级标题、居中、嵌套样式、多行内容和源码编辑检查。
功能标记反馈示例：[用户提供的完整内容](../../fixtures/html-functional-markers.md)。Logo 测试资源复制自本机 `D:/study/neutrino-proxy-client-desktop/assets/logo/logo-text-cross.png`，保留相对路径 `assets/logo/logo-text-cross.png`；远程徽章地址保持用户原文。可运行 `./scripts/run-windows.ps1 -Document './fixtures/html-functional-markers.md'` 检查。
Windows 可运行 `./scripts/run-windows.ps1 -Document './fixtures/html-compatibility.md'`；macOS 可运行 `cargo run --locked -p tiny-md -- fixtures/html-compatibility.md`。

解析器接口依据：[html5gum DefaultEmitter](https://docs.rs/html5gum/0.8.4/html5gum/emitters/default/struct.DefaultEmitter.html)；块级 HTML 规则依据：[CommonMark HTML blocks](https://spec.commonmark.org/0.31.2/#html-blocks)。
