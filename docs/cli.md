# CLI 安装与使用

CodeMori CLI 是可独立运行的原生命令行工具，也是 VS Code、JetBrains 插件与 Skill 调用的业务核心。**使用预编译 CLI 不需要安装 IDE、Rust、Node.js、Python 或启动后台服务。** 只有安装 Skill 的桥接脚本需要 Python 3。

它适合终端用户、脚本集成和 AI 工具调用。当前提供 `info`、`init`、`rpc` 三个子命令；资料管理、检索、关联和备份通过 JSON RPC 调用。

## 下载哪个文件

在 [GitHub Releases](https://github.com/Lusyne/CodeMori/releases) 中选择与你的操作系统和 CPU 一致的附件。

| 平台 | CLI 完整包（推荐） | 独立二进制 |
|---|---|---|
| macOS Apple Silicon | `codemori-cli-0.1.0-macos-arm64.tar.gz` | `codemori-cli-0.1.0-macos-arm64` |
| macOS Intel | `codemori-cli-0.1.0-macos-x86_64.tar.gz` | `codemori-cli-0.1.0-macos-x86_64` |
| Linux arm64 | `codemori-cli-0.1.0-linux-arm64.tar.gz` | `codemori-cli-0.1.0-linux-arm64` |
| Linux x64 | `codemori-cli-0.1.0-linux-x86_64.tar.gz` | `codemori-cli-0.1.0-linux-x86_64` |
| Windows x64 | `codemori-cli-0.1.0-windows-x86_64.zip` | `codemori-cli-0.1.0-windows-x86_64.exe` |

完整包包含程序、使用说明、Skill、项目 MIT 许可证。

## macOS / Linux

以 macOS Apple Silicon 完整包为例，下载后解压并进入目录：

```sh
tar -xzf codemori-cli-0.1.0-macos-arm64.tar.gz
cd codemori-cli-0.1.0-macos-arm64
./codemori --version
./codemori info
```

若选择的是独立二进制，则运行：

```sh
chmod +x ./codemori-cli-0.1.0-macos-arm64
./codemori-cli-0.1.0-macos-arm64 info
```

Linux 或 Intel Mac 替换为表格中对应的文件名。可保留原名直接执行，或复制并命名为 `codemori` 放进自己的 PATH 目录。无需用管理员权限运行 CodeMori。当前 macOS 包未签名；系统阻止打开时先核对来源及 SHA256，不要全局关闭系统安全检查。

## Windows PowerShell

完整包示例：

```powershell
Expand-Archive .\codemori-cli-0.1.0-windows-x86_64.zip -DestinationPath .
cd .\codemori-cli-0.1.0-windows-x86_64
.\codemori.exe --version
.\codemori.exe info
```

直接下载 `.exe` 时可运行 `.\codemori-cli-0.1.0-windows-x86_64.exe info`。PowerShell 中当前目录的程序需要 ` .\ ` 前缀，不必先添加 PATH。

## 检查完整性

下载附件对应的 `.sha256`，或 Release 中的总 `SHA256SUMS`，比对计算结果。macOS 可用 `shasum -a 256 文件名`，Linux 可用 `sha256sum 文件名`，PowerShell 可用 `Get-FileHash 文件名 -Algorithm SHA256`。校验和用于检查下载是否完整；分发平台与发布者仍应是你信任的来源。

## 三个基本命令

| 命令 | 作用 | 是否写入数据 |
|---|---|---|
| `codemori --help` / `--version` | 帮助 / 版本 | 否 |
| `codemori info` | 以 JSON 返回版本、协议、数据目录等 | 否，不创建目录 |
| `codemori init` | 初始化或迁移个人资料库 | 是 |
| `codemori rpc` | 从标准输入读取一条 JSON 请求并执行 | 取决于操作 |

默认个人数据目录为 `~/.codemori/`；Windows 通常为 `C:\Users\你的用户名\.codemori\`。同机 IDE 和 CLI 使用默认目录时访问同一份资料。示例与测试建议显式指定隔离目录：

```sh
./codemori --data-dir ./codemori-demo info
./codemori --data-dir ./codemori-demo init
```

第一条不会创建目录，第二条才会初始化。不要将示例资料目录提交到代码仓库。

## 保存并搜索一个片段

以下 Unix shell 示例使用隔离资料库：

```sh
printf '%s' '{"protocol_version":1,"request":{"op":"record_create","record":{"kind":"snippet","title":"重试判断","content":"return error.type === \"timeout\";","language":"typescript","tags":["重试"]}}}' | ./codemori --data-dir ./codemori-demo rpc

printf '%s' '{"protocol_version":1,"request":{"op":"search","filter":{"query":"重试","limit":20}}}' | ./codemori --data-dir ./codemori-demo rpc
```

也可把一条请求保存为 UTF-8 的 `request.json`：

```sh
./codemori --data-dir ./codemori-demo rpc < request.json
```

PowerShell 用管道输入，显式设置 UTF-8 以保留中文：

```powershell
$OutputEncoding = [System.Text.UTF8Encoding]::new($false)
Get-Content .\request.json -Raw -Encoding utf8 | .\codemori.exe --data-dir .\codemori-demo rpc
```

每次调用接收一个 JSON 对象，并在输入结束后执行。它不是交互式终端，也不是常驻 RPC 服务。

## 能调用哪些功能

| 需求 | RPC 操作示例 |
|---|---|
| 管理片段和文档摘要 | `record_create`、`record_update`、`search` |
| 关联项目文件与文档 | `workspace_register`、`document_link`、`file_documents` |
| 团队资料与署名 | `project_save`、`library_search`、`identity_set` |
| 文档回跳源码 | `code_link_create`、`code_link_resolve` |
| 代码变化与复核 | `binding_changes`、`binding_review`、`bindings_review` |
| 备份与恢复 | `backup_export`、`backup_preview`、`backup_import` |
| AI 读取证据 | `code_search`、`code_read`、`ai_context` |

参数结构、版本冲突规则和返回值见 [JSON RPC 协议](cli-protocol.md)。程序化集成应调用 CLI，不要直接修改内部 SQLite 或跳过共享版本检查。

## 返回值与错误

`info`、`init` 和 `rpc` 返回 JSON，业务操作包含 `protocol_version`、`ok` 和 `data` 或 `error`。同时检查退出码：`0` 成功，`1` 业务/存储/JSON 错误，`2` 命令行参数错误。帮助和版本输出是普通文本。

CLI 搜索已保存的资料字段；源码搜索和读取是独立操作。它不自动下载飞书、Notion、Obsidian 正文，也不执行 Git 提交或推送。复核会写入新的基线，不能把普通查询误当成复核操作。

## Skill

需要让 AI 查找代码并生成说明时，下载 CLI 完整包，按 [AI Skill 指南](ai-skill.md)安装。它调用同一个 CLI 获取证据，由当前 AI 助手理解和写作。