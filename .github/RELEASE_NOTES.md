# CodeMori 0.1.0 开发预览

代码在哪，文档就在哪。

将代码与文档关联，把设计思路、团队经验、复用片段，嵌入开发工作流。

[在线文档](https://lusyne.github.io/CodeMori/) · [快速开始](https://lusyne.github.io/CodeMori/quickstart.html) · [兼容性说明](https://lusyne.github.io/CodeMori/compatibility.html)

## 本版功能

- VS Code 与 JetBrains 的代码片段、文档关联、搜索和收藏。
- 文件/目录继承、可隐藏的代码旁提示、代码变化复核与当前文件一键确认。
- 飞书客户端入口、文档回跳代码、Git 共享与资料署名。
- AI 知识 Skill：检索代码和关联资料，生成有源码依据的说明草稿。
- JetBrains 最低支持 2024.2，不设最高版本限制；新版优先使用新 API，旧版保留适配分支。已通过 IDEA 2024.2、2026.1 与 RustRover 2026.2 的 API 兼容检查。

## 选择安装包

- `.vsix`：VS Code 扩展；`codemori-版本-平台.zip`：JetBrains 插件。
- `codemori-版本-jetbrains-universal.zip`：JetBrains 通用插件，内置下列五个平台的核心程序，可直接从磁盘安装。
- `codemori-cli-版本-平台.tar.gz`（Windows 为 `.zip`）：CLI 完整包，含原生程序、AI Skill 和许可证。
- `codemori-cli-版本-平台`（Windows 为 `.exe`）：可直接下载的独立 CLI 二进制，无需安装 Rust、Node.js 或 Python；Unix 下载后需 `chmod +x`。
- 独立二进制不包含 Skill；需要 AI Skill 时选择 CLI 完整包。用法见 [CLI 安装与使用](https://github.com/Lusyne/CodeMori/blob/main/docs/cli.md)。
- 平台包括 macOS arm64/x86_64、Linux arm64/x86_64、Windows x86_64。除 JetBrains 通用包外，请匹配 OS/CPU；使用 `SHA256SUMS` 检查完整性，安装方法见[下载指南](https://lusyne.github.io/CodeMori/download.html)。

这是开发预览，安装包尚未签名。插件市场提交、审核和上架是独立步骤，可用状态以各市场页面为准。CI 原生测试/包校验不等于所有平台完整桌面验收，具体范围以[兼容性说明](https://lusyne.github.io/CodeMori/compatibility.html)为准。更新前备份个人资料，团队共同升级兼容版本。
