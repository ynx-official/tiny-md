# 输入光标与正文同步

- 状态：Review
- 最后更新：2026-10-09
- 关联文档：[渲染性能优化](render-performance-optimization.md)、[验证记录](../verification.md)

## 问题与修复

用户反馈快速输入中文和英文时，光标不能稳定跟随新输入的内容。
GPUI 渲染回归确认：连续输入使长段落换行并触发自动滚动时，旧实现画出的光标与
平台输入接口返回的文本插入点纵向相差 12 px；该回归中的文本顺序和逻辑插入位置仍正确。
这项证据确认了显示错位，尚不能证明用户报告的所有输入错乱均来自同一原因。

旧实现根据正文 render 时的滚动偏移计算光标；GPUI 随后在 prepaint 中约束实际滚动位置，
正文与光标因此使用不同的几何信息。现在由正文的 bounds 探针在 prepaint 中使用实测内容原点
更新光标，并由光标层在 paint 时读取最终坐标、颜色和显隐状态。保留正文缓存与独立光标闪烁，
普通编辑、组合输入、选区和撤销语义继续由原有模型负责。

## 验证依据

- `rapid_input_paints_the_caret_at_the_latest_insertion_point` 在修复前实际失败：光标 y=152 px，
  文本插入点 y=164 px；修复后通过。测试连续送入多个原生输入回调后绘制一帧，
  覆盖英文、中文、emoji、换行和段落增长触发的自动滚动，核对正文、UTF-16 插入位置及实际 paint 坐标。
- `composition_and_source_mode_keep_the_painted_caret_with_the_text` 通过：带边框编辑器中，
  普通模式与源码模式均覆盖预编辑、组合选区、中文提交、随后输入英文及两次撤销；每帧光标与插入点一致。
- `./scripts/check.ps1` 通过：17 项 Python 发布工具测试、当前版本资料校验、Rust 格式检查、
  139 项工作区测试（应用 38、文档 16、编辑器 73、更新器 12）及 Clippy；原有一个 doctest 忽略。
- 光标独立刷新、滚动坐标、正文缓存工作量和已有 UTF-16 / 输入法回归继续通过。
- `./scripts/bundle-windows.ps1 -Portable` 成功：release / x64 MSVC / 静态 CRT，
  Windows GUI 子系统与九个嵌入图标帧校验通过。`./scripts/update-windows-local.ps1`
  已更新本机安装，复制前后 SHA-256 一致，保留备份
  `tiny-md-before-7a48183638c74ae88bf0770be1fe22c7.exe.bak`。
  更新脚本不关闭现有文档窗口；重新从快捷方式启动后使用修复版。

自动化使用 GPUI TestPlatform 的布局、输入回调和 paint 路径，不等同于 Windows GPU、
真实键盘高速输入或系统中文输入法候选框的验收。开发阶段归入 `Unreleased`，没有发布或替换公开附件。
现纳入 `v0.2.2`；本次正式发布按用户要求跳过测试，证据见 [版本详情](../06-delivery/versions/v0.2.2.md)。
