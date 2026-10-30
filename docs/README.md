# 文档索引

- 状态：Review
- 最后更新：2026-10-30
- 关联文档：[项目说明](../README.md)

| 文档 | 内容 |
|---|---|
| [UI 风格](02-design/ui-style.md) | 已确认的 notion 风格、原生应用适配、侧栏虚线调宽与正文 / 侧栏滚动条 |
| [偏好设置与更新窗口](02-design/preferences.md) | 文件菜单入口、三档夜间模式与系统主题跟随、设置持久化、后台下载和更新窗口重新排版 |
| [更新线路设计思路与详细方案](02-design/update-routes.md) | 设计取舍、页面交互、自定义线路逐条增删与迁移、探测与选择、签名信任、下载切换和交付边界 |
| [更新线路验证](04-quality/update-routes.md) | 线路选择、签名、下载切换、配置和 Windows 原生验证证据 |
| [更新发布签名](05-operations/update-signing.md) | Ed25519 密钥配置、签名附件、发布顺序与过渡规则 |
| [正文排版偏好](02-design/typography.md) | 正文宽度、字体、字号、行高与段落间距调节、多窗口同步、默认值与源码保护 |
| [应用图标](02-design/application-icons.md) | Windows 留白与 DPI 尺寸、macOS 独立素材、导出及资源核对 |
| [外部文件同步](02-design/external-file-sync.md) | 自动同步、三方合并、冲突处理与撤销保护 |
| [文档窗口](02-design/document-windows.md) | 侧栏定位当前文档、默认目录折叠、当前窗口切换、空白未命名窗口复用与草稿保护 |
| [最近文件记录](02-design/recent-files.md) | 本地持久化、20 条去重排序、路径展示、清除及文件操作同步 |
| [在线更新设计](03-architecture/online-updates.md) | 已确认的三级更新源、下载校验、安装重启与发布边界 |
| [ELK 流程图布局](03-architecture/flowchart-elk-layout.md) | 原生 ELK 适配、分组与坐标、字体、回退及资源保护 |
| [流程图阅读顺序](04-quality/flowchart-reading-order.md) | 多循环图主线顺序、外围回退线、分支引号与 Typora 截图对照 |
| [Mermaid 解析兼容性](04-quality/mermaid-parser-compatibility.md) | 大写 END、带标签链式连线、完整转交样例、错误保留与 35 图复用 |
| [在线更新验证](04-quality/online-updates.md) | 更新与发布工具回归、真实下载检查、平台验收边界 |
| [Windows 安装版更新修复](04-quality/windows-installed-update.md) | 旧版重开根因、路径兼容、安装版助手与失败恢复回归 |
| [Windows 适配](05-operations/windows.md) | Windows 开发环境、运行、快捷键及验收边界 |
| [拖放与 Windows 文件集成](05-operations/windows-shell-integration.md) | 拖放打开、右键入口、默认打开勾选项及重装 / 卸载清理 |
| [验证记录](verification.md) | 自动检查、原生交互和输入法验收证据 |
| [输入光标同步](04-quality/input-caret-synchronization.md) | 快速输入、换行与自动滚动的光标坐标修复及组合输入回归 |
| [Markdown 交互](04-quality/markdown-interactions.md) | 复选框原位勾选、链接手形光标、表格与换行链接、只读与源码模式边界 |
| [Markdown 表格快捷操作](04-quality/markdown-table-controls.md) | 表格斑马纹、工具栏、尺寸网格与数字输入、缩小确认、更多菜单、键盘导航、源码与撤销保护及双平台验证边界 |
| [图片内嵌预览](04-quality/inline-images.md) | 本地相对路径与 HTTP/HTTPS、后台加载、编辑 / 源码切换、资源回收与跨平台验证边界 |
| [SVG 字体内存优化](04-quality/svg-font-memory.md) | 按需字体读取、紧凑的流程图字体缓存、像素一致性及原生进程内存对照 |
| [HTML 混排兼容](04-quality/html-compatibility.md) | HTML 标题、换行与注释标记、徽章横排与链接、折叠与 HTML 表格、源码保存及中文输入 / 撤销回归 |
| [段落空行间距](04-quality/paragraph-spacing.md) | Windows / macOS 共用的分隔空行高度、连续空行与源码边界、应用配置与编辑缓存回归 |
| [正文滚动条](04-quality/document-scrollbar.md) | 窗口最右侧窄滚动条、内容长度与滚动位置、渲染缓存同步及模式 / 主题回归 |
| [大纲章节定位](04-quality/outline-navigation.md) | 双向跳转顶端对齐、已可见章节重定位、源码 / 打字机模式与文末范围 |
| [长文档渲染性能](04-quality/render-performance-analysis.md) | 实测开销、卡顿来源与优化顺序 |
| [渲染性能优化](04-quality/render-performance-optimization.md) | 五项优化实现、缓存回归与原生复测边界 |
| [流程图内存与独立查看](04-quality/diagram-memory-and-viewer.md) | 取消闲置缓存、活动图片弱引用共享、整篇预生成、最多 5 并发、骨架与高清图回收及内存对照 |
| [流程图加载性能](04-quality/diagram-load-performance.md) | 开发版首次加载的分段计时、ELK / 字体 / 编译优化与性能对照边界 |
| [开发路线](roadmap.md) | 现有功能与后续范围 |
| [版本构建](releases.md) | macOS 与 Windows 安装包构建、优化配置及发布流程 |
| [版本总览](06-delivery/versions/index.md) | 已发布历史与分版本唯一正文 |
| [v0.2.9 版本详情](06-delivery/versions/v0.2.9.md) | ELK 与流程图加载优化、Mermaid 兼容修复及本次跳过测试的发布记录 |
| [v0.2.10 版本详情](06-delivery/versions/v0.2.10.md) | 流程图闲置缓存回收与多循环主线布局优化 |
| [v0.2.11 版本详情](06-delivery/versions/v0.2.11.md) | 大纲章节定位与侧栏当前文档定位 |
| [v0.2.13 版本详情](06-delivery/versions/v0.2.13.md) | Markdown 本地与远程图片内嵌预览 |
| [v0.2.14 版本详情](06-delivery/versions/v0.2.14.md) | Markdown HTML 混排、可见功能标记及 SVG 字体内存优化 |
| [v0.2.15 版本详情](06-delivery/versions/v0.2.15.md) | Markdown 表格编辑工具、夜间模式与持久化最近文件 |
| [v0.2.16 版本详情](06-delivery/versions/v0.2.16.md) | 正文宽度、字体、字号、行高和段落间距偏好 |
| [v0.2.17 版本详情](06-delivery/versions/v0.2.17.md) | 更新线路自动择优、手动测速、固定/自定义线路与签名清单 |
| [v0.2.12 未发布记录](99-archive/v0.2.12-failed-release.md) | Clippy 检查失败与补丁版本替代 |
| [v0.2.8 未发布记录](99-archive/v0.2.8-failed-release.md) | CI Clippy 失败、未公开 Release 与替代版本 |
| [v0.2.7 版本详情](06-delivery/versions/v0.2.7.md) | 双平台段落空行修复与本次跳过测试的发布记录 |
| [v0.2.6 版本详情](06-delivery/versions/v0.2.6.md) | Windows 段落空行间距修复与本次跳过测试的发布记录 |
| [v0.2.5 版本详情](06-delivery/versions/v0.2.5.md) | 侧栏当前窗口切换与简洁更新日志的发布记录 |
| [v0.2.4 版本详情](06-delivery/versions/v0.2.4.md) | 复选框原位勾选、交互光标、表格与换行链接及该次跳过测试的发布记录 |
| [v0.2.3 版本详情](06-delivery/versions/v0.2.3.md) | 分类偏好设置、更新页重设计、后台自动下载与该次跳过测试的发布记录 |
| [v0.2.2 版本详情](06-delivery/versions/v0.2.2.md) | Windows 在线更新与输入光标修复、窗口 / 侧栏改进及该次跳过测试的发布记录 |
| [v0.2.1 版本详情](06-delivery/versions/v0.2.1.md) | 独立窗口、文件集成与本次跳过测试的发布记录 |
| [v0.2.0 版本详情](06-delivery/versions/v0.2.0.md) | 在线更新等历史正式发布范围与验证记录 |
| [变更日志](../CHANGELOG.md) | 用户摘要与未发布变化 |
| [MIT 许可证](../LICENSE) | 项目开源许可与版权声明 |
