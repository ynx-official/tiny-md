# 拖放笔记与 Windows 文件集成

- 状态：Review
- 最后更新：2026-10-08
- 关联文档：[Windows 适配](windows.md)、[文档窗口](../02-design/document-windows.md)、[版本构建](../releases.md)、[变更日志](../../CHANGELOG.md)

## 拖放打开

文档窗口接收 GPUI 的外部文件拖放。一次可拖入一篇或多篇 `.md`、`.markdown`、`.mdown` 笔记，
扩展名不区分大小写。空列表或包含其他扩展名的拖放不打开；目录、不可读或无效 UTF-8 文件由原有异步读取流程报告错误。

拖放复用统一的独立窗口打开流程，保留当前笔记的内容与未保存状态，打开失败不会覆盖当前编辑器。
忙碌、退出或安装更新期间不接受拖放。此实现不把文件复制到应用目录，也不修改拖入的源文件。

## 安装入口

Windows 安装器继续使用当前用户安装，无需管理员权限，始终为 `.md` 与 `.markdown` 注册：

- 右键菜单“通过 Tiny MD 打开”，图标取自安装程序的可执行文件。
- “打开方式”中的 Tiny MD 和 Windows 默认应用列表中的 Tiny MD。
- 带引号的程序路径及文件参数，支持路径包含空格和中文。

Windows 11 的传统右键扩展位于“显示更多选项”；“打开方式”仍由系统提供。
这是本轮静态 Shell verb 的入口，见 [微软的 Windows 11 右键说明](https://blogs.windows.com/windowsdeveloper/2021/07/19/extending-the-context-menu-and-share-dialog-in-windows-11/)。

## 默认打开勾选项

安装任务新增 `.md` 与 `.markdown` 两个独立选项，首次安装默认不勾选；重装时沿用 Inno Setup 的上次任务选择。
勾选后，完成页提示把所选扩展名的打开方式设为 Tiny MD，点击“完成”后打开 Windows 默认应用设置。
Windows 11 使用 Tiny MD 的应用专属设置链接；不支持此链接的旧系统需在总页选择 Tiny MD，Windows 10 打开默认应用总页。

Windows 的默认应用由用户在系统中确认，安装器注册可选应用，不写入受系统管理的 `UserChoice` 和其 Hash，
也不调用 Windows 8 以后不再支持的默认应用设置接口。
依据：[默认应用接口支持范围](https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nn-shobjidl_core-iapplicationassociationregistration)、
[默认应用设置页](https://learn.microsoft.com/zh-cn/windows/apps/develop/launch/launch-default-apps-settings)。
静默安装及在线更新跳过打开设置页，保留用户已选择的默认打开方式。

## 重装与卸载

| 注册区域（HKCU） | 安装 / 重装 | 卸载 |
|---|---|---|
| `Software\Classes\SystemFileAssociations` 下两种扩展名的 `TinyMD.Open` verb | 更新同一个入口及程序路径 | 删除 Tiny MD 的 verb 及子项 |
| `Software\Classes\TinyMD.Markdown`、`Applications\tiny-md.exe` | 注册程序命令与图标 | 删除 Tiny MD 的专属键 |
| 两种扩展名的 `OpenWithProgids` | 写入 `TinyMD.Markdown` 值 | 只删除该值；空键才删除 |
| `Software\Tiny MD\Capabilities` | 注册支持的文件类型 | 删除 Tiny MD 的能力信息 |
| `Software\RegisteredApplications` | 写入 `Tiny MD` 值 | 只删除该值 |

所有命令重新安装时更新到当前 `{app}`，不创建重复菜单。`ChangesAssociations=yes` 在安装和卸载结束时通知资源管理器刷新。
清理使用 Inno Setup 的卸载日志和专属键 / 值删除标记，不删除扩展名默认值、整个共享扩展名键或其他应用的注册。
依据：[Registry 卸载标记](https://jrsoftware.org/ishelp/topic_registrysection.htm)、[关联刷新](https://jrsoftware.org/ishelp/topic_setup_changesassociations.htm)。
若用户之前把 Tiny MD 选为默认应用，卸载后由 Windows 处理该选择，必要时需重新选择默认应用。

## 验证与边界

本轮使用工作区内的便携 Inno Setup 6.7.3 编译安装脚本；编译工具未注册为系统安装软件。
发布流水线的 Windows 安装验收增加了首次安装、带空格 / 中文路径的重装、右键 / 打开方式注册、
卸载清理以及原默认值 / `UserChoice` 保持不变的检查，检查脚本为 `scripts/verify-windows-shell.ps1`。
该脚本只读取注册表，不自行安装程序或修改关联。

本轮实际通过：格式检查、应用全部 target 的 Clippy（`-D warnings`）、Windows x64 静态运行库的
最高优化 release 构建、GUI 子系统 / 图标资源核验，以及安装脚本的 Inno Setup 编译。
新增 PowerShell 校验脚本和流水线安装代码通过语法解析，版本资料一致性及 diff 检查通过。

开发安装包为 `target/shell-integration-build/tiny-md-unreleased-windows-x64-setup.exe`，
大小 10,312,362 字节；使用独立暂存目录中的本轮 release 程序、版本文件、欢迎文档和图标编译。
程序 SHA-256 为 `CB896A9C8498F35D1E5F1841CF7F610C3D247AE7AA5138B777E0AA0147E1A2EE`，
安装包 SHA-256 为 `7465DDFB5A389046D19667CB5A12EC5C5C31DFA5974ACE03D092617E45AE005F`。

按本轮沿用的跳过测试要求，未执行安装 / 卸载验收和原生资源管理器 / 拖放 / 默认应用交互；
不得把脚本编译通过视为这些系统交互已验收。macOS 的原生拖放也尚未实机确认。
本轮改动最初归入 `Unreleased`，随后纳入 [v0.2.1](../06-delivery/versions/v0.2.1.md)；
不修改或替换已发布 `v0.2.0` 的附件。本次发布验证以版本详情中的实际记录为准。
