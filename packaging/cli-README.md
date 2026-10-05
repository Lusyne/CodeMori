# CodeMori CLI

此压缩包包含文件名所标注系统与CPU架构的原生可执行文件。无需安装 Rust、系统SQLite或独立桌面客户端。IDE插件已经携带核心，一般IDE用户不需要另装CLI。

## 开始使用

解压后运行 `./codemori info`，Windows使用 `.\codemori.exe info`。它以JSON输出版本与数据路径，不创建资料目录。默认目录为 `~/.codemori/`，可用 `--data-dir /absolute/path` 指定其他资料库。

- `info`：只读运行信息。
- `init`：创建或升级个人资料库。
- `rpc`：从标准输入读取一条UTF-8 JSON请求，输入结束后执行。

Unix shell示例：

```sh
printf '%s' '{"protocol_version":1,"request":{"op":"search","filter":{"query":"Redis 重试"}}}' | ./codemori rpc
```

失败响应包含 `ok:false`，同时检查退出码。旧核心会拒绝较新格式的数据库，因此应一起升级两端插件。迁移前使用 `backup_export` 备份，不要直接覆盖使用中的SQLite文件。

## 当前版本能力

0.1.0使用协议1、个人数据库schema4、备份导出2/读取1和2、共享文件写入3/读取1到3。共享资料位于 `<项目>/.codemori/shared.json`，通过Git单独备份；CLI不执行Git提交、拉取或推送。

支持片段与文档摘要、文件/目录关联、署名、代码位置链接、变更复核和路径修复。`bindings_review` 按单个资料库原子确认一批关联；`code_search`、`code_read`、`ai_context` 提供有范围限制的源码和知识证据。它们不会自动发布文档或确认复核。

## AI Skill

压缩包内含 `skills/codemori-knowledge/`。安装桥接脚本需要Python3，在解压目录运行：

```sh
python3 skills/codemori-knowledge/scripts/install.py
```

安装器复制Skill、匹配平台核心及项目许可证。CLI程序自身不需要Python。工具只提供证据，由当前AI助手生成说明草稿；远程文档正文不会自动读取。

## 校验与说明

使用旁边的 `.sha256` 或Release总 `SHA256SUMS` 检查完整性。压缩包包含项目MIT许可证。

[中文安装与使用](https://github.com/Lusyne/CodeMori/blob/main/docs/cli.md) · [JSON RPC协议](https://github.com/Lusyne/CodeMori/blob/main/docs/cli-protocol.md)

当前为开发预览，实际平台验证范围以仓库兼容性说明为准。Release另提供不含Skill文件的独立二进制。
