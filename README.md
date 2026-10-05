# CodeMori

[在线文档](https://lusyne.github.io/CodeMori/) · [功能简介](docs/overview.md) · [快速开始](docs/quickstart.md) · [CLI 工具](docs/cli.md) · [GitHub](https://github.com/Lusyne/CodeMori)

**代码在哪，文档就在哪。**

将代码与文档关联，把设计思路、团队经验、复用片段，嵌入开发工作流。

CodeMori 是本地优先的开源 IDE 知识连接工具。你可以在 VS Code 和 JetBrains 中保存代码片段，为文件或目录关联已有文档，并在需要时找回实现背后的说明，无需单独启动 CodeMori 客户端。

## 适用场景

- **接手项目**：打开源码就能看到关联的设计说明、接口约定和业务规则，减少来回寻找文档的时间。
- **日常开发**：把飞书、Notion、Obsidian 或本地 Markdown 关联到文件和目录，在 IDE 中查看摘要、打开原文。
- **代码复用**：保存常用实现、排错方法和配置片段，配上标题、标签与使用说明，需要时搜索并复制。
- **团队交接**：将适合共享的片段、摘要和文档关联随 Git 提交，新成员拉取项目后即可查看，资料保留创建者与修改者署名。
- **维护设计说明**：代码修改后检查关联文档是否仍然适用，复核后一次确认当前文件的待复核项。

## 核心特点

- **双 IDE 使用**：提供 VS Code 和 JetBrains 插件，同机共用个人资料库，插件自带原生核心。
- **代码与文档双向连接**：从 IDE 打开原文，将代码位置链接贴回文档；平台拦截链接时，可在 IDE 粘贴打开。[使用说明](docs/code-links.md)
- **接入现有文档**：关联飞书、Notion、Obsidian 和本地 Markdown。支持的飞书文档可在客户端打开，本地 Markdown 提供只读预览。[飞书入口](docs/feishu-opening.md)
- **模块知识继承**：文件和目录都可关联文档，子目录中的源码继承模块说明；代码旁提示可按需显示或隐藏。[关联与提示](docs/contextual-knowledge.md)
- **变更复核与一键确认**：基于已保存代码提示说明待复核，确认后更新关联基线；并发变更会拒绝旧确认。[复核操作](docs/ai-skill.md)
- **随 Git 共享**：个人与项目共享范围明确区分，共享资料保留创建者与修改者，沿用团队的提交和审查流程。[团队指南](docs/project-sharing.md)
- **多种知识来源**：搜索已保存片段和手填摘要，支持中文关键词；可手动索引 Java、JS/TS 和 Python 源码注释。[索引说明](docs/indexing.md)
- **本地存储与备份**：个人资料默认保存在 `~/.codemori/`，支持 JSON 备份、导入预览与工作区重新定位。[数据说明](docs/data-privacy.md)
- **独立 CLI 与 AI Skill**：原生命令行工具支持 `info`、`init` 和 JSON RPC；Skill 向 AI 提供源码与知识证据，不要求使用指定模型。[CLI](docs/cli.md) · [Skill](docs/ai-skill.md)

## 快速开始

1. 按操作系统、CPU 和 IDE 选择[安装包](docs/download.md)。VS Code 使用“从 VSIX 安装”；IntelliJ IDEA 使用 **Plugins → Install Plugin from Disk**。
2. 打开源码，选中一段代码并右键保存片段，或为当前文件/目录关联文档，填写一句摘要。
3. 在 CodeMori 面板查看当前文件的关联说明，或运行 `CodeMori: 搜索资料` 找回已保存内容。
4. 需要团队共用时，选择“项目共享（随 Git）”，检查并提交项目中的共享文件。

安装插件无需 Rust、Node.js 或数据库配置。CLI 可脱离 IDE 独立使用；AI Skill 桥接脚本额外需要 Python 3。完整步骤见[第一次使用](docs/quickstart.md)。

## 从源码开发

Rust 核心需要 Rust/Cargo 和 C 编译器；VS Code 扩展与文档站需要 Node.js 22，JetBrains 插件需要 JDK 21。

```sh
# 在仓库根目录构建并检查核心
cargo test --workspace --locked
cargo build --release --locked -p codemori-cli

# 查看运行信息，不创建用户资料目录
cargo run -p codemori-cli -- info
```

IDE 插件的构建、测试和打包步骤见[开发说明](docs/development.md)。测试使用临时目录或显式的 `--data-dir`，避免操作真实个人资料。

```text
crates/codemori-core/   资料管理、搜索与共享业务核心
crates/codemori-cli/    原生命令行工具与 JSON RPC
plugins/jetbrains/     JetBrains 插件
plugins/vscode/        VS Code 扩展
skills/                CodeMori AI 知识 Skill
scripts/               构建产物校验与发布辅助脚本
packaging/             CLI 发行包说明
docs/                  用户指南与 VitePress 文档站
```
## 许可证

采用 [MIT 许可证](LICENSE)。
