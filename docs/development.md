# 从源码开发

## 环境要求

- Rust1.85或更新版本、Cargo和C编译器；依赖由 `Cargo.lock` 固定。
- VS Code 扩展与文档站：Node.js22、npm。
- JetBrains 插件：JDK21；仓库自带 Gradle Wrapper，编译 SDK 为 IntelliJ IDEA Community 2024.2，以最低支持版本检查 API。

首次下载依赖需要联网。使用产品安装包不需要这些开发工具。

## 工程边界

业务逻辑、路径检查、SQLite、搜索及共享资料管理位于 `crates/codemori-core/`；`crates/codemori-cli/` 提供版本化JSON边界。IDE适配器处理编辑器交互和展示，不直接打开SQLite。协议变更时同步检查两个适配器和[协议说明](cli-protocol.md)。

产品Skill位于 `skills/codemori-knowledge/`，属于发行内容；不是开发者个人工具配置。默认用户资料在 `~/.codemori/`，测试必须使用临时目录。

## Rust 与 CLI

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo build --release --locked -p codemori-cli
python3 scripts/package-cli.py
```

`target/release/codemori` 为本机可执行文件（Windows为 `.exe`）。CLI包生成到 `target/dist/`，按[CLI指南](cli.md)测试 `info` 和RPC。`info` 不创建个人数据；测试业务调用时显式使用 `--data-dir`。

## VS Code

```sh
cd plugins/vscode
npm ci
npm test
npm run package
```

输出本机平台 `.vsix`，可通过“从VSIX安装”使用。需要实际IDE宿主测试时，先确认可以启动独立测试窗口，再执行 `CODEMORI_ALLOW_IDE_TEST=1 npm run test:host`。它使用独立配置和测试库；普通编译与单元测试不需要打开IDE。

## JetBrains

```sh
cd plugins/jetbrains
./gradlew --no-daemon test buildPlugin verifyPluginStructure
```

Windows使用 `gradlew.bat`。ZIP输出位于 `build/distributions/`，包含本机平台核心。可选 `-PlocalIdePath=/path/to/IDE` 使用匹配版本的本地SDK。`runIde` 会启动沙箱窗口，不能作为无界面检查使用。

## 文档开发

```sh
cd docs
npm ci
npm run dev
```

修改Markdown保存后网页自动更新。首页在 `index.md`，导航在 `.vitepress/config.mts`，配色与布局在 `.vitepress/theme/`。完成后执行：

```sh
npm test
npm run build
npm run check
npm run check:preview
```

`npm run preview` 用于查看生产构建，修改后需先重新构建；它读取构建记录的部署路径。仅编辑文档时不需要重跑Rust和IDE测试。

## 发行与许可证

发行包携带项目的 `LICENSE`，不再包含单独的第三方许可汇总文件。CLI包说明位于 `packaging/cli-README.md`。

GitHub Actions分别承担构建测试、文档部署和标签触发Release。具体执行步骤见 `.github/workflows/`；Release读取 `.github/RELEASE_NOTES.md` 作为发布说明，发布前请更新该文件。CI配置的存在不代表目标平台已验收；发布时准确记录实际通过的范围。
