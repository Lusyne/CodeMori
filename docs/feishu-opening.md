# 在飞书中打开文档

在 CodeMori 中保存或关联普通飞书文档链接，选中文档后点击 **在飞书中打开**。IDEA 与 VS Code 都会尝试唤起系统注册的飞书客户端，以独立窗口打开该页面；旁边保留 **浏览器打开**。

- 首批识别 HTTPS 的 `*.feishu.cn` 文档地址，包括 `doc`、`docs`、`docx`、`wiki`、`sheets`、`base`、`mindnotes`、`slides`、`file` 和 `drive/file` 路径。链接需包含文档标识，非默认端口及其他域名仍走普通原文入口。
- 原始 URL 不被替换。复制链接、导出备份、去重和已有文件关联保持原有语义；打开前重新校验记录版本。
- 需要安装飞书并具备对应文档的访问权限。没有客户端、未登录或没有权限时，请使用浏览器入口或在飞书中处理登录／权限；插件不会代为授权。
- 按钮显示“已请求飞书打开”只代表发出请求，不代表远端文档一定加载成功。没有自动切换到浏览器，避免重复打开窗口。
- 此功能只导航，不读取云文档正文，也不需要配置 App ID、App Secret 或 API token。

## 官方协议与桌面限制

官方[云文档 AppLink](https://open.feishu.cn/document/common-capabilities/applink-protocol/supported-protocol/open-docs)明确标注 `/client/docs/open` 不支持 PC。因此桌面插件使用支持 PC 的[网页打开协议](https://open.feishu.cn/document/common-capabilities/applink-protocol/supported-protocol/open-the-web-view-in-feishu-to-access-the-specified-url)：`/client/web_url/open`，模式为 `window`，目标 URL 作为独立参数编码。

根据[AppLink 结构说明](https://open.feishu.cn/document/common-capabilities/applink-protocol/applink-introduction/applink-structure)，`feishu://` 可尝试从外部直接唤起客户端；无法唤起时不会自动进入 HTTPS 引导页。CodeMori 保留普通浏览器按钮作为备用入口。