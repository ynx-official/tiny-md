# 原生 ELK 流程图布局

- 状态：Review
- 最后更新：2026-10-14
- 关联文档：[加载性能](../04-quality/diagram-load-performance.md)、[缓存与调度](../04-quality/diagram-memory-and-viewer.md)、[UI 风格](../02-design/ui-style.md)

## 范围与选择

用户要求继续优化多流程图文档的首次打开，接入 ELK，保留内存缓存、最多 5 个计算任务与按文档顺序显示。
同日后续确认速度改善，明确取消闲置缓存并继续保留整篇预生成位图；此选择替代此前会话缓存要求。
目标是接近 Typora 的秒级体验；验收区分 CPU 生成计时与实际原生窗口显示，不能以诊断计时代替界面验收。

沿用 Rust / GPUI / Guise。新增固定版本 `elkrs 0.1.1`，其发布包声明 Apache-2.0，
许可证保留于 `crates/editor-adapter/LICENSE.elkrs`。这是第三方 Rust ELK 移植，
不是直接调用 Mermaid.js；不增加 Node、浏览器进程或在线渲染服务。
API 与上游范围见 [elkrs 文档](https://docs.rs/elkrs/0.1.1/elkrs/)和
[上游仓库](https://github.com/depetrol/elkrs)。兼容性以本项目回归与实际样本为准。

流程图默认使用 ELK layered 布局；源码明确指定 `flowchart.defaultRenderer` 为
`dagre`、`dagre-d3` 或 `dagre-wrapper` 时继续使用原生布局。其他图类型保留既有算法。
ELK 返回错误、panic 或不完整 / 非有限几何时回退原生布局，后续仍执行既有 SVG 尺寸与光栅化保护。
布局后的节点排列和图形长宽可以变化；源码、样式、主题和编辑历史不因布局改变而改写。

## 适配边界

`diagram_layout.rs` 将引擎选择和坐标适配集中在编辑器模块内，`diagrams::render` 仍是统一生成入口。
解析入口经过 `mermaid_parse.rs` 的兼容处理，大写 END 与带标签链式连线的边界见
[解析兼容性](../04-quality/mermaid-parser-compatibility.md)。
先从解析图移除连线，调用既有布局器取得文字换行、节点尺寸、形状、样式、链接和分组标题。
这避免执行旧布局器最昂贵的连线路由和标签避让；边标签另按唯一文本测量一次。

ELK 接收有尺寸的节点、分组层次及正交连线。内部 ID 独立生成，源码 ID 只参与映射，
同一进程的每个计算任务拥有自己的引擎实例，不引入串行引擎锁。
输入仅为 JSON 数据，不执行源码中的脚本或外部命令。

分组由最小包含关系建立嵌套树，节点属于最内层分组；子节点和连线坐标转为整图坐标。
节点位置采用 SVG 渲染器要求的左上角坐标，边标签锚点采用中心坐标。
分组预留标题高度和最小宽度，叶节点关闭继承的自动尺寸约束，避免圆形被分组标题拉宽。
连线终点投影至菱形、圆形等实际形状边界，保持多端口箭头与节点连接。
分组别名节点隐藏，指向分组的连线使用分组边界。

保留上下左右四个全图方向。没有节点跨边界连线的子图保留显式方向，使用独立子图布局；
有节点连接外部时沿用整体方向，分组之间的连线仍可连接分组别名。
源码中的 `elk.nodePlacementStrategy` 只接受
`SIMPLE`、`NETWORK_SIMPLEX`、`BRANDES_KOEPF`、`LINEAR_SEGMENTS`；
`elk.forceNodeModelOrder` 只接受布尔值。随机种子固定为 1；其他未经适配的配置不透传。

## 主流程与回退连线

多循环流程图使用 `elk.layered.cycleBreaking.strategy = DEPTH_FIRST`。
先从无入边的起点沿有向连线遍历，按源码连线顺序处理分支；返回当前遍历路径的连线作为回退边。
没有起点的闭环按节点出现顺序开始遍历。这样避免默认贪心策略因为后段决策的出边较多，
把主流程末段移到起点上方；不按节点名称、P0 等编号或标签内容猜测业务阶段。
有向无环图继续按依赖关系分层，提前声明终点不会把流程倒排。

开启 `elk.layered.feedbackEdges`，使回退边绕过节点；
设置 `elk.layered.considerModelOrder.strategy = NODES_AND_EDGES`，在减少交叉的约束下参考源码顺序。
不强制锁定每一层的节点顺序，仍允许 ELK 优化分支与交叉；显式 `forceNodeModelOrder` 配置保留。
适配仍使用正交连线、原有字体与主题，不要求与 Typora 的坐标或折线逐像素一致。
遍历顺序是布局启发式，复杂图中存在多条可选主路径时不宣称能够推断作者的业务优先级。
方案、完整样例与验证见[流程图阅读顺序](../04-quality/flowchart-reading-order.md)。
配置含义参考 [ELK 环路策略](https://eclipse.dev/elk/reference/options/org-eclipse-elk-layered-cycleBreaking-strategy.html)、
[回退边](https://eclipse.dev/elk/reference/options/org-eclipse-elk-layered-feedbackEdges.html)和
[模型顺序](https://eclipse.dev/elk/reference/options/org-eclipse-elk-layered-considerModelOrder-strategy.html)。

## 性能与资源

开发 profile 对所有第三方依赖使用优化级别 2，工作区代码保持开发优化级别与调试符号 / 断言。
这覆盖字体、SVG、几何与 GPUI 之间的依赖调用；首次切换配置会重编译依赖。
Windows 图中文字使用 `Microsoft YaHei, Segoe UI, sans-serif`，避免先请求 macOS 字体后反复查找中文回退字体。
macOS 沿用原有字体栈，字号与深浅色配色不变。

`diagram_raster.rs` 继续使用 GPUI 已依赖的 usvg 0.45.1 / resvg 0.45.1 和相同 SVG 光栅化参数，
仅为流程图提供独立的共享字体选项。原 GPUI 解码器的文件型字体会在字形读取中反复映射字体文件，
因此将选定的系统字体常规 / 粗体 / 斜体文件读入共享字节，跨图与窗口复用。
实际路径来自系统字体数据库，不硬编码 Windows 或 macOS 字体文件路径；不下载或打包字体。
字体字节总预算为 64 MiB，超过预算、读取失败或解析失败的字体保留原有文件读取。
这项共享只包含本机系统字体，不包含文档文字，退出进程时释放；取消闲置图形缓存后继续保留，避免字体读取再次拖慢生成。
初始化在已有后台计算任务内完成，后续每个任务共享只读字体数据，独立创建 SVG 树和像素缓冲。
生成的像素、预乘透明度与 GPUI 既有 SVG 分支逐字节对照；没有新增界面线程的解码工作。

ELK 可以生成比旧布局更长的合法图。SVG 逻辑边长保护由 12,000 调整为 128,000；
源码 64 KiB、200 节点保护与正文 / 高清的像素预算保留。
扩大逻辑画布不扩大纹理或像素缓冲上限；长图仍按比例限制解码尺寸。

SVG 与正文位图只用弱引用登记正在使用的结果，注册表不持有像素数据；错误由当前编辑器保存。
活动文档仍准备并保留全部位图，后台计算继续共用 5 个名额。关闭文档、替换或调整尺寸后，
没有其他使用者的旧数据释放；重开已关闭的文档重新生成，仍在其他窗口使用的结果可共享。
高清查看保持单独分配与关闭释放。算法随进程固定，不跨进程复用旧版本布局。
缓存、骨架和取消行为的权威规则见关联文档，性能结果与原生验收边界见加载性能记录。
