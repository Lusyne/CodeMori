# 插件下载
<ReleaseDownloads />

## 选择哪种包
**由于内部打包了二进制文件，所以只能下载对应架构的扩展**

**插件已经包含核心程序，无需安装 Rust、Python 或独立客户端。** Skill 的 Python 要求仅适用于 Skill。

| 系统 / CPU | VS Code 扩展 | JetBrains 插件 |
|---|---|---|
| macOS Apple Silicon | `codemori-0.1.0-darwin-arm64.vsix` | `codemori-0.1.0-macos-arm64.zip` |
| macOS Intel | `codemori-0.1.0-darwin-x64.vsix` | `codemori-0.1.0-macos-x86_64.zip` |
| Linux arm64 | `codemori-0.1.0-linux-arm64.vsix` | `codemori-0.1.0-linux-arm64.zip` |
| Linux x64 | `codemori-0.1.0-linux-x64.vsix` | `codemori-0.1.0-linux-x86_64.zip` |
| Windows x64 | `codemori-0.1.0-win32-x64.vsix` | `codemori-0.1.0-windows-x86_64.zip` |

## VS Code

1. 打开扩展视图，点击顶部“…” → **从 VSIX 安装**。
2. 选择匹配平台的 `.vsix`，按提示重新加载。
3. 打开一个本地项目，在命令面板执行 **CodeMori: 搜索资料**。
4. 选中一段源码，右键执行 **CodeMori: 保存选中代码**。

详见[VS Code 指南](vscode.md)。

## JetBrains

1. Settings / Preferences → Plugins → 齿轮 → **Install Plugin from Disk**。
2. 选择匹配平台的 JetBrains `.zip`，保持压缩包原样。
3. 按提示重新加载，使用 Find Action 执行 **CodeMori: 搜索资料**。

详见[JetBrains 指南](jetbrains.md)。

## CLI 二进制工具与 AI Skill

命令行用户可选 `codemori-cli-0.1.0-<平台>.tar.gz`（Windows 为 `.zip`）完整包，或直接下载同名独立二进制（Windows 带 `.exe`）。无需安装 IDE 或 Rust。完整的安装、校验、命令及 JSON RPC 示例见 [CLI 安装与使用](cli.md)。全部文件名见[下载说明](download.md)。CLI 包包含可安装的 `codemori-knowledge` Skill；[安装方法](ai-skill.md#安装-skill)需要 Python 3。

## 更新与卸载

更新时安装匹配平台的新包，并一起升级同机两端插件、团队使用的共享格式及 Skill 核心。先导出个人备份，团队资料由 Git 备份。

卸载插件不会删除 `~/.codemori/` 或项目共享文件。重新安装可继续使用；清理前请先阅读[数据与备份](data-privacy.md)。

## 从源码构建

按[开发说明](development.md)构建对应平台插件。
