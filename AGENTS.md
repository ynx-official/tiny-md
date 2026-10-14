# tiny-md 项目约定

## 技术与范围

- 沿用 Rust workspace、GPUI 0.2.2、GPUI Component 0.5.0、Guise 1.9.1，不更换技术栈。
- `crates/app` 管理窗口、菜单和文件交互；`crates/editor-adapter` 管理编辑与平台输入；`crates/document` 管理磁盘状态。
- Windows 适配以 Windows 10/11 x64 + MSVC 为目标，保留现有 macOS 行为和简洁写作布局。
- 新平台特有代码应限制在明确的平台边界内；公共行为应有回归测试。
- 用户于 2026-10-13 明确段落分隔空行修复必须统一应用于 Windows 与 macOS；两端共用半行分隔规则，标题留白、字体和窗口布局继续沿用各平台既有设置。
- 用户于 2026-10-14 要求流程图默认按文档从头到尾渲染，加载时使用骨架预留空间，避免滚动条拖动期间加载导致阅读位置不断跳动；继续保留屏外图后台预生成。
- 用户于 2026-10-14 要求复用未变更的流程图，并选择先用内存缓存；缓存覆盖同次运行中切换 / 重开文档，图源码变化只更新对应图，独立高清查看仍在关闭后释放。
- 用户于 2026-10-14 补充不再逐张串行生成，最多同时执行 5 个流程图计算任务；布局、正文解码及高清查看跨窗口共用该总上限，结果继续按文档顺序显示。
- 2026-10-14 用户确认使用 `cargo run` 并提供实际文档计时；开发构建优化第三方依赖，工作区代码保留开发调试配置；诊断方法及原生计时边界见 `docs/04-quality/diagram-load-performance.md`。
- 用户于 2026-10-14 要求接入 ELK 并继续争取秒级体验；流程图使用原生 Rust ELK 适配，保留内存缓存、共用 5 个计算名额和按序显示。设计见 `docs/03-architecture/flowchart-elk-layout.md`；CPU 计时不得代替原生窗口或 Typora 同机计时。

## UI 风格

- Skill: `awesome-design-md`
- Source: `VoltAgent/awesome-design-md`
- Style ID: `notion`
- Style reference: `C:/Users/Administrator/.codex/skills/awesome-design-md/references/design-md/notion/DESIGN.md`
- 用户于 2026-10-08 确认；应用范围与原生字体、深色主题的适配说明见 `docs/02-design/ui-style.md`。
- 保留现有正文与 macOS 布局，将该风格用于 Windows 窗口内操作入口及后续 UI。
- 用户于 2026-10-08 补充 Typora Windows 截图：Windows 使用左侧应用标题、顶部七项菜单和底部视图按钮，无文件启动时显示空白文档；截图要求优先于风格库通用布局。

## 验证与文档

- 功能修改先复现测试失败，再实现并运行相关测试。
- macOS：`sh scripts/check.sh`。
- Windows：`./scripts/check.ps1`；该脚本自动加载 Visual Studio C++ x64 与 Windows SDK 环境。
- 无实际运行证据时，不声明原生界面、输入法或发布安装包已验收。
- 正式说明统一放在 `docs/`，同步维护 `docs/README.md`。
- 未经用户明确要求，不提交、推送、创建 PR 或发布版本。

## 在线更新与发版

- 更新实现位于 `crates/updater`，GPUI 交互位于 `crates/app/src/updates.rs`；参考 `D:/study/ashell` 的流程与 schema 1，独立实现并保留本项目许可证。
- 版本唯一来源是根 `Cargo.toml` 的 `workspace.package.version`；界面、更新器与版本探测使用编译版本，不硬编码。
- `CHANGELOG.md` 只记录精简摘要；版本详细正文统一在 `docs/06-delivery/versions/vX.Y.Z.md`，总览为同目录 `index.md`。
- 新功能先归入 `Unreleased`。准备发版时同步 workspace、四个 workspace 包的锁文件版本、版本详情、总览、CHANGELOG 日期及链接。
- 标签必须精确匹配 Cargo 版本，发版前运行 `python scripts/release_notes.py --check-current`。
- Release Notes 从版本详情生成，排除工程验证与变更依据；更新清单、正文、安装包和便携包齐全后才能从草稿公开。
- 用户于 2026-10-09 要求更新日志只写本次实际变化，不重复旧版本缺少在线更新、需要先手动安装等历史提示，也不逐版重复通用安装步骤、平台介绍和未变化的限制。只有本版本新增的迁移、兼容性或操作要求才进入用户正文；测试与发布核对证据放在“验证结果”。
- 禁止替换已公开版本的附件；失败重跑的公开产物不一致时必须使用新版本。
- 自动检查默认不自动下载；用户于 2026-10-09 要求新增可选后台自动下载，开启后下载与校验成功才弹出更新页。禁止自动安装。安装助手只在全部文档窗口通过未保存检查后启动；取消、保存失败必须保留窗口。
- 开发构建只支持手动检查和下载，禁止安装更新；Windows 发布包额外运行 `scripts/test-windows-update.ps1` 的隔离助手验收。
- 架构与验收见 `docs/03-architecture/online-updates.md` 和 `docs/04-quality/online-updates.md`。
