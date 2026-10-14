# 流程图首次加载性能

- 状态：Review
- 最后更新：2026-10-14
- 关联文档：[文档索引](../README.md)、[缓存与调度](diagram-memory-and-viewer.md)、[Windows 运行](../05-operations/windows.md)

## 输入与测量范围

用户确认运行方式是 `cargo run`，并指定以下文档用于计时：
`D:\project\蓝蔚士项目分析v2\docs\02-design\end-to-end-flow-diagrams.md`。
文件为 89,266 字节、35 个 Mermaid 块，与之前的测试副本内容一致。
测试前后 SHA-256 为 `0F4C05EAB625F10DF388D3D0091894F3566F1734E9920368BA9C56BD3359C98E`。
原文件未修改；34 张有效图完成生成，第 23 张保留既有的语法错误。

诊断直接调用项目的 `diagrams::render` 与 GPUI SVG 解码路径，逐图重新生成。
显示宽度固定为 1000 逻辑像素、DPI 为 200%，保留现有边长和像素预算保护。
不读取应用的 SVG / 位图缓存；底层字体库自然保留同一进程内的初始化与字体复用。
`layout_ms` 包含 Mermaid 解析、布局与 SVG 输出，`pixels_ms` 包含目标画布调整和 SVG 到像素的转换。
成功生成后另重复解析源码，用于区分解析与后续生成成本，该重复解析不计入前述两项总和。

默认诊断逐张执行，便于计算每段开销；5 线程诊断用独立线程分发全部图形，最多 5 个 CPU 任务同时运行。
后者没有模拟应用的按序发布、文档排版、窗口绘制、GPU 上传与缓存；不是原生首屏、全部图显示时间或 FPS。
阶段总和是累计任务时间，在多线程下不能当作墙钟时间。构建时间不计入生成时间。
本轮没有实际测量 Typora，也没有完成 Windows / macOS 原生窗口计时。

## 实测结果

| 同一份完整文档 | 未优化开发依赖，串行 | 优化开发依赖，串行 |
|---|---:|---:|
| Mermaid 解析、布局及 SVG 生成合计 | 78.706 秒 | 12.335 秒 |
| SVG 到位图合计 | 30.749 秒 | 12.348 秒 |
| 两段计算总和 | 109.456 秒 | 24.684 秒 |
| 成功 / 错误数量 | 34 / 1 | 34 / 1 |
| 全部目标像素字节合计 | 251.97 MiB | 251.97 MiB |

两段计算总和减少约 77.4%。这是本机单次对照，不是任意文档的固定加速倍数。
像素字节是全部图目标缓冲的合计；诊断逐图释放结果，不代表进程峰值或活动文档缓存占用。

优化依赖后的 **5 线程 CPU 诊断墙钟时间为 7.989 秒**；全部 34 张有效图生成完成。
累计 SVG 生成为 17.185 秒、累计位图生成为 19.096 秒。并发时单个任务可能因 CPU 和字体资源竞争变慢。
第 2 张在该并发诊断中累计耗时约 7.816 秒，是整个任务完成时间的重要约束。

最慢的第 2 张有 144 个节点、142 条边。
未优化串行配置下，该图 SVG 生成约 17.495 秒、位图生成约 4.001 秒；重复解析源码约 16 毫秒。
优化后的串行 SVG 生成约 3.149 秒、位图生成约 2.203 秒；重复解析约 5 毫秒。
证据说明源码读取 / 解析不是主要等待来源，单张复杂图的 SVG 与位图生成仍需继续优化。
SVG 生成阶段包含布局和文字测量；本轮没有继续细分两者耗时，不能将这一阶段全部归到某个内部步骤。

## 已实施的修正

根 `Cargo.toml` 对 Mermaid、resvg、usvg、tiny-skia、fontdb、rustybuzz、ttf-parser
设置开发依赖 `opt-level = 2`。普通 `cargo run` 会采用这项配置；工作区代码仍保留开发调试配置。
首次切换配置会重新编译相关依赖，已运行的旧进程需要退出后重新启动。
测试 profile 继承开发配置，因此相同计算路径可在诊断测试里对照。
正式发行 profile 原本已有优化，本次开发配置的提升不能直接写成发行版加速。

没有更换渲染引擎、降低图形像素规格或改变缓存、5 个任务上限、按序显示和屏外预生成的既有行为。
诊断入口属于显式启用的 ignored 测量工具，不使用不稳定的墙钟断言，也不进入应用运行路径。

## 与 Typora 的区别及剩余瓶颈

[Typora 官方说明](https://support.typora.io/Draw-Diagrams-With-Markdown/)确认其集成 Mermaid。
[Mermaid API](https://mermaid.js.org/config/usage.html)输出 SVG；Tiny MD 当前采用另一个 Rust 原生渲染器，
随后用 GPUI / resvg 生成完整正文位图。这两条生成链路的算法、字体处理和像素工作量不同。
没有 Typora 同机计时或内部调用证据，不能把速度差全部归到某个未经验证的 Typora 优化策略。

该文档第 2 张指定了 `defaultRenderer: elk`。
Tiny MD 当前用 `LayoutConfig::default()` 计算 `parsed.graph`，没有将 `parsed.init_config` 切换为 ELK 引擎。
[Mermaid 的 ELK 说明](https://mermaid.js.org/syntax/flowchart.html)提供的是 Mermaid.js 的另一种布局能力，
不能据此认为当前 Rust 原生渲染器已使用同一算法。

应用仍按要求准备所有屏外图的完整正文位图；这增加了首次打开所需的 CPU 与像素工作。
按序显示还要求后面的结果等待前面的慢图。缓存主要改善同次运行中的复用，首次未命中仍要计算。
后续提速重点是复杂图的布局 / 文字测量、位图生成和结果发布等待；当前证据尚未证明秒级原生打开。

## 复测方式

在 Windows 项目目录执行：

```powershell
. ./scripts/setup-windows.ps1
$env:TINY_MD_DIAGRAM_DOCUMENT = 'D:\project\蓝蔚士项目分析v2\docs\02-design\end-to-end-flow-diagrams.md'
$env:TINY_MD_DIAGRAM_PROFILE_WORKERS = '5'
cargo test --locked -p tiny-md-editor profile_diagram_generation -- --ignored --nocapture
```

线程数可设为 1–5；省略 `TINY_MD_DIAGRAM_PROFILE_WORKERS` 时默认为 1。
`TINY_MD_DIAGRAM_PROFILE_LIMIT` 可限制取样图数量，省略时处理全部图。
诊断不输出图中文字或源码，只输出图序号、节点 / 边数量、尺寸与计时。
原始日志在忽略目录 `target/diagram-profile-unoptimized.log`、`target/diagram-profile-optimized.log`
和 `target/diagram-profile-five-workers.log`。

## 完成前验证

`./scripts/check.ps1` 通过：17 项发布工具测试、版本元数据、格式、190 项 Rust 测试
（应用 64、文档 16、编辑器 95、更新器 15）及 Clippy。
新诊断默认 ignored，已通过本轮显式串行 / 并行运行验证；原有一个示例 doctest 继续忽略。
用户指定文档的完整生成及缓存重开回归通过，35 个结果与首次加载一致，34 张有效图的位图 ID 保持一致；
重开时全部 5 个计算名额被占住，确认直接复用。该 TestPlatform 回归用时 23.35 秒，不是原生窗口计时。
`cargo build --locked -p tiny-md` 的 Windows MSVC 开发构建通过。
文件 SHA-256 保持不变；未替换安装程序、提交、推送或发布。
