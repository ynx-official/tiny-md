# 正文滚动条

- 状态：Review
- 最后更新：2026-10-14
- 关联文档：[UI 风格](../02-design/ui-style.md)、[渲染缓存](render-performance-optimization.md)、[验证记录](../verification.md)

## 范围与实现

用户要求打开长 Markdown 时在最右侧显示窄滚动条，能够判断内容长度与阅读位置。
视觉与交互规则以 [UI 风格](../02-design/ui-style.md) 为准。

编辑器适配层提供共享 `ScrollHandle`，应用层将已有 GPUI Component 滚动条放在窗口正文区域右缘。
正文与滚动条共用顶部留白及工具栏底部留白，正文宽度与居中方式保持既有规则。
滚动条拥有独立 GPUI view；它通知滚动位置变化时，应用使编辑器缓存失效，
以重建可见行与同步光标 / 输入范围坐标。仅实际偏移变化触发正文刷新，普通悬停不会重建正文。
不增加依赖、不更换编辑器、不修改 Markdown 内容或撤销历史，双平台共用实现。

2026-10-14 根据用户的流程图骨架要求，滚动条 handle 增加拖动状态，与编辑器共用。
开始 / 结束拖动即使偏移没有变化，也通知正文刷新；按住滑块时冻结流程图高度和既有错误展开状态。
松开后修正尺寸并保持当前阅读行，位图生成不改变文档高度。详细规则见
[流程图加载](diagram-memory-and-viewer.md)。

## 回归验证

前两项回归先运行并失败：长文档未找到窗口边缘滚动条；实现后通过。
测试通过真实 GPUI TestPlatform 布局和鼠标事件路径执行，不强制刷新缓存来掩盖拖动问题。

| 场景 | 断言 |
|---|---|
| 1000 / 1800 px 窗口，侧栏与工具栏开关 | 滚动条紧贴正文区域最右缘，操作区域不超过 16 px，上下对齐滚动视口 |
| 拖动、正文滚轮、点击轨道 | 能到达文末及返回文首，正文与输入范围坐标同步，内容与未保存状态不变 |
| 40 段增长至 160 段，再切换短文档 | 滑块随内容增多缩短；短文档无可滚动范围，点击原滚动区域不会移走内容 |
| 浅色 / 深色、排版 / 源码 | 轨道操作使用同一正文滚动状态，原文保持一致 |

`cargo test --locked -p tiny-md document_scrollbar -- --nocapture`：4 项通过。

`./scripts/check.ps1` 通过：174 项工作区测试、17 项 Python 发布工具测试、
当前版本资料校验、Rust 格式检查及 `cargo clippy --locked --workspace --all-targets -- -D warnings`；
原有一个示例 doctest 忽略。

Windows 开发版构建通过。当前 `target/debug/tiny-md.exe` 正在运行，构建复用已有临时 target，
验证用程序复制至 `target/scrollbar-preview/tiny-md.exe`，复制前后 SHA-256 一致。
`scripts/verify-windows-exe.ps1` 确认 x64 Windows GUI 子系统与九个嵌入图标帧符合项目约定。

## 验收边界

流程图加载补充回归覆盖真实滑块按住 / 松开、加载期间滚动范围固定、位图完成高度不变和已有错误展开保留。
本次完整检查与开发构建通过，最新数量和加载样本证据见 [流程图回归](diagram-memory-and-viewer.md)。

自动化使用 GPUI TestPlatform，不等同于 Windows GPU 绘制或 macOS 原生验收。
当前会话未进行原生窗口的视觉检查、多 DPI 或系统输入法验收；未替换安装版或发布新版本。
