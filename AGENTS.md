# tiny-md 项目约定

## 技术与范围

- 沿用 Rust workspace、GPUI 0.2.2、GPUI Component 0.5.0、Guise 1.9.1，不更换技术栈。
- `crates/app` 管理窗口、菜单和文件交互；`crates/editor-adapter` 管理编辑与平台输入；`crates/document` 管理磁盘状态。
- Windows 适配以 Windows 10/11 x64 + MSVC 为目标，保留现有 macOS 行为和简洁写作布局。
- 新平台特有代码应限制在明确的平台边界内；公共行为应有回归测试。

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
