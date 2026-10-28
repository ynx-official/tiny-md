# 更新清单发布签名

- 状态：Review
- 最后更新：2026-10-29
- 关联文档：[线路设计](../02-design/update-routes.md)、[发布流程](../releases.md)、[验证记录](../04-quality/update-routes.md)

## 密钥与发布

`crates/updater/update-public-key.hex` 是客户端内置的 Ed25519 公钥。私钥仅用于生成清单签名，不放入仓库、产物或文档。
发布工作流需要仓库 Actions Secret `TINY_MD_UPDATE_SIGNING_KEY`，内容为 PKCS#8 私钥文件的十六进制编码。
发布验证任务在密钥缺失时停止；签名工具还检查私钥对应的公钥与客户端内置值完全一致。
2026-10-29 发版准备前已将该私钥配置到仓库 Actions Secret `TINY_MD_UPDATE_SIGNING_KEY`；Secret 列表确认配置存在，私钥值未回显或写入仓库。

首次配置时，把私钥文件通过受控方式写入该 Secret，避免在命令行参数、日志或聊天中打印值。保留受访问控制的离线备份。
本机私钥保存在用户 Codex 目录下独立的 `keys/tiny-md-update-signing-20261028/` 目录，目录 ACL 仅授权当前用户；该路径不会打包或上传。

如需在新的项目环境从零建立签名对，可使用：

```text
cargo run --locked -p tiny-md-updater --example sign_manifest -- --generate-key PRIVATE_FILE PUBLIC_FILE
```

此命令拒绝覆盖私钥和非空公钥。Unix 新私钥权限为 0600；Windows 必须先给存放私钥的目录配置仅当前用户可访问的 ACL。
已有用户安装了内置旧公钥的应用后，不得直接替换公钥；应先通过可信旧发布链交付能够识别新公钥的客户端，再切换签名密钥。本次没有实现在线密钥轮换。

## 产物协议

清单继续使用 schema 1，旧客户端忽略新增 `notes_digest` 字段，安装包地址保持原始 GitHub 地址。
新增附件 `update-manifest.json.sig`，是对 `update-manifest.json` 原始 UTF-8 字节的 Ed25519 签名，编码为 128 个十六进制字符和末尾换行。
签名后不能改换行、重新格式化或修改清单。日志哈希使用实际文件字节计算。

流水线按顺序生成日志和清单、签名并自校验、生成覆盖签名附件的 SHA256SUMS、上传全部草稿附件，确认完整后再公开。
公开产物共 8 个：两个平台的 4 个安装 / 便携包、日志、清单、签名和 SHA256SUMS。
不得给已经公开的旧版本补传或替换签名；新协议随新版本发行。

```text
cargo run --locked -p tiny-md-updater --example sign_manifest -- MANIFEST
cargo run --locked -p tiny-md-updater --example sign_manifest -- --verify MANIFEST
```

签名命令从 `TINY_MD_UPDATE_SIGNING_KEY` 环境变量读取私钥，验证命令只需要内置公钥。
旧版无签名清单仍需 GitHub 直连核验；签名过渡没有取消 TLS、附件长度、SHA-256、平台选择或用户确认安装的规则。
