# 更新线路验证

- 状态：Review
- 最后更新：2026-10-28
- 关联文档：[线路设计](../02-design/update-routes.md)、[发布签名](../05-operations/update-signing.md)、[在线更新验证](online-updates.md)

## 自动验证

本次先运行旧配置与线路 UI 的失败回归，再实现线路功能；发布工具的日志哈希及缺少签名附件场景也先确认失败。

`./scripts/check.ps1` 通过：340 个 Rust 测试、19 个 Python 测试，版本元数据检查、rustfmt、整个 workspace 的所有 target Clippy（`-D warnings`）通过。
Rust 分布为 app 106、document 16、editor-adapter 191、updater 单元 19、updater 集成 8；原有 2 个手动性能测试和 1 个文档测试仍标记 ignored。
本机完整日志为 `target/update-routes-check.log`，不提交构建产物。

新增与扩展的覆盖：

| 边界 | 验证内容 |
|---|---|
| 线路选择 | 三路并发、延迟排序、固定模式、仅 GitHub 不访问代理、旧版本缓存排除 |
| 元数据 | HTML 响应、清单不一致、无签名代理在 GitHub 失败时不可用、官方备用接口核验已有代理响应 |
| 发布签名 | 有效签名独立核验、篡改字节、错误公钥、无效编码、签名错误不得降级、可信来源同版本冲突拒绝 |
| 下载 | HTTP 503 可恢复切换、每条线路最多一次、取消与完整性错误不切换 |
| 配置和状态 | 旧设置兼容、保存失败不应用、更新中配置锁定、旧测速结果失效、不改检查历史、不弹更新窗、不改变在途传输 |
| UI | 默认尺寸线路控件、深浅色最小 640 × 430 布局、固定完成按钮、滚动可访问检查更新按钮 |
| 发布工具 | 清单新增日志哈希、允许签名附件参与产物核对、发布前缺少签名即停止 |

## Windows 原生与实际网络

原生开发版使用独立的 `target/update-ui-smoke` 配置目录，保持用户已安装应用的设置不变。
实际观察默认 900 × 700 页面、浅色和深色主题、测速中状态、三条线路响应时间与当前线路、全部模式下拉选项、自定义输入与保存 / 取消控件。
手动测速前后核对配置文件：不会创建或改写更新检查历史。点击检查更新会打开更新页并显示线路检测和结果。
最小尺寸使用 GPUI 窗口测试验证；本次没有把该测试视作原生拖拽缩放验收。

通过 updater 示例执行真实请求及下载（耗时只代表当次网络样本）：

- 自动线路一轮返回 GitHub 3349 ms、gh-proxy.com 2661 ms、ghfast.top 超时，选择 gh-proxy.com。
- 通过该线路完整下载 v0.2.16 Windows 安装包，共 12,143,085 字节，大小与 SHA-256 验证成功。临时文件释放，未执行安装包。
- 指定 ghfast.top 的另一轮探测为 1835 ms；真实下载在接收数据后取消，返回取消结果，未安装。
- 原生页面另一轮同时显示三路成功，依据该轮结果选择最快线路。网络变化会改变排序，不能把示例数值当作性能保证。
- 最终重新构建后再次运行原生手动测速：GitHub 3928 ms、gh-proxy.com 1740 ms、ghfast.top 超时，界面正确高亮 gh-proxy.com 为当前使用。

可复用命令：

```text
cargo run --locked -p tiny-md-updater --example routes -- --download
cargo run --locked -p tiny-md-updater --example routes -- --ghfast --cancel-download
```

这两个示例只测试下载、校验或取消，不启动安装助手。

## 验收边界

`cargo build --locked -p tiny-md` 构建通过。本地 `sign_manifest` 工具签名与独立 `--verify` 验签均通过；签名烟测只在 `target/` 内生成本地副本，不上传或修改已有公开版本。
本次没有运行 macOS 原生窗口、正式安装包助手验收或 GitHub Actions 发布流程；共用逻辑通过本机测试，不据此声明这些环境已验收。
仓库签名 Secret 尚未配置。下一次正式发布前需完成[签名配置](../05-operations/update-signing.md)，随后按既有发布流程验证安装助手及产物。
未提交、推送、打标签或发布。
