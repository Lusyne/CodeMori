# CLI 协议 1

运行 `codemori rpc`，向标准输入写入一条UTF-8 JSON请求后关闭输入。可选的全局参数 `--data-dir PATH` 用于隔离资料库。单条请求最多128MiB，核心操作不会发起网络连接。

```json
{"protocol_version":1,"request":{"op":"search","filter":{"query":"Redis 重试","limit":50}}}
```

标准输出返回一个响应对象：包含 `protocol_version:1`、布尔值 `ok`，以及成功时的 `data` 或失败时的 `error:{code,message}`。退出码0表示成功，1表示业务/存储/JSON错误，2表示命令行语法错误。`info` 不访问存储；`init` 与需要存储的业务RPC会初始化或打开资料库。

## 基础操作

| op | 参数 | data |
|---|---|---|
| workspace_register | root（现有目录），可选name | Workspace，复用规范化后的根目录 |
| workspace_list | 无 | Workspace[] |
| workspace_relocate | id, root, revision | 更新后的Workspace，保留稳定ID |
| record_create | record: RecordInput | Record；相同文档URL复用现有记录 |
| record_get | id | Record |
| record_update | id, revision, record: RecordInput | 更新后的Record |
| record_delete | id, revision | deleted:true；级联删除关联，不删除外部文件 |
| search | 可选filter | items:[{record,workspace_names,excerpt:[{text,highlight}]}], total,limit,offset |
| tags | 可选demo:false | 已有标签的展示名称 |
| document_link | binding:{workspace_id,path,document_id} | 规范化的关联，重复调用保持幂等 |
| document_unlink | binding | unlinked:true；保留文档记录 |
| file_documents | workspace_id,path | 已关联的Record[] |
| document_bindings | document_id | 该文档的全部文件关联 |
| binding_move | binding, path（新相对路径） | 原子修复后的关联，文档不变 |
| source_target | source:{workspace_id,path,line?} | 已存在的规范化本地路径，拒绝越出工作区 |
| document_target | id, revision | 重新校验后的文档URL；拒绝过期版本和不支持的参数 |
| document_open_targets | id, revision | {original_url,feishu_applink:null或字符串}；派生打开选项，不修改存储 |
| markdown_preview | 本地Markdown文档的id, revision | title,path,html；转义后的只读HTML，不加载远程资源 |
| comments_index | workspace_id，可选相对path；null表示工作区 | IndexReport；手动增量索引已保存文件 |
| comments_status | workspace_id | files,comments,indexed_at；无索引时为null |
| comments_search | 可选filter:{query,workspace_id?,limit:50,offset:0} | items:[{source,end_line,text,language,workspace_name,indexed_at,excerpt}],total,limit,offset |
| comments_target | source | 本地路径；拒绝索引后内容变化的目标 |
| backup_export | 无 | Backup，当前导出格式2 |
| backup_preview | backup | 导入报告，不写入知识记录 |
| backup_import | backup | 原子导入报告，冲突保留本地版本 |

内置示例加载/移除接口已移除。`is_demo` 和底层查询中的 `demo` 字段仅为兼容历史资料保留，两端界面不再提供示例入口。

## 数据类型

**Workspace**：`id,name,root,revision`。

**Record**：`id,revision,created_at,updated_at,is_demo,input`。时间戳为Unix毫秒；revision从1开始，更新/删除必须携带读取时的版本。收到 `CONFLICT` 后应重新加载并复核，不得静默用新版本重试旧修改。

**RecordInput**：`kind`（snippet/document）、`title`、`content`、`language`、`description`、`tags`、`starred`、`source`、`url`。可选字段默认空值、false或null。文档的description是手填摘要，content为空、starred为false、source为null，通过独立Binding关联源码。片段的url必须为null，source可选，由工作区ID、相对路径和可选的1起始行号组成。片段标题为空时自动生成，语言可从扩展名推断。标签去除首尾空白，忽略大小写去重并保留展示拼写。

**SearchFilter**：`query`，可选 `kind,tag,workspace_id`，默认 `starred:false,demo:false,limit:50,offset:0`。limit范围1到200。关键词按空白拆分并使用AND匹配，进行Unicode小写归一化的子串搜索；`%` 和 `_` 按字面量处理。排序依次为标题/标签精确命中、标题/标签子串命中、描述/正文命中，同分按最近更新和稳定ID排序。只搜索已保存字段，不搜索远程正文。excerpt直接返回原文片段与highlight布尔值，避免Java和JavaScript对UTF-8/UTF-16偏移量理解不一致；适配器应按文本或转义HTML显示。

**Backup**：`format_version,exported_at,workspaces,records,bindings`，当前导出2，读取1和2。插入前校验全部ID、URL、路径、时间戳、重复键和引用。报告字段为 `new_workspaces,new_records,new_bindings,unchanged,conflicts:[{kind,id,reason}],invalid_count,invalid_entries:[{kind,index,id,reason}]`。先预览，再由用户确认导入同一份数据；导入事务重新计算冲突，已有本地修改优先保留。

## 存储与升级

当前schema为4。升级旧数据库前，核心使用SQLite在线快照生成 `migration-v<旧版本>-<时间戳>-<uuid>.sqlite3.bak`；迁移与版本写入位于同一事务，失败回滚，拒绝打开更高版本。SQLite启用外键、WAL和5秒忙等待；超时返回 `STORAGE_ERROR`，不能伪报成功。

文档地址允许不含凭据的HTTP(S)、`obsidian://open`、本地文件URI或绝对路径。适配器必须用IDE文件API打开本地文件，不能作为可执行程序启动。跨系统备份保留工作区根路径，供用户手动重新定位。

Obsidian只允许导航参数 `vault`、`file`、`path` 和可选的 `paneType=tab|split|window`。authority必须为open，不能带凭据、端口或进一步的动作路径。拒绝其他参数，包括经过百分号编码的append、prepend、content、clipboard、overwrite及回调参数。CodeMori不通过“打开原文”暴露写入功能。参见[官方URI说明](https://help.obsidian.md/Extending+Obsidian/Obsidian+URI)。

两端打开文档前都用选中记录的ID/revision调用 `document_target`，只使用返回URL。它也校验历史记录，但不自动重写或删除；用户可通过编辑修正旧链接。片段返回 `VALIDATION_ERROR`，不存在的ID返回 `NOT_FOUND`，过期版本返回 `CONFLICT`。适配器须随包携带匹配核心，不能在操作不可用时退回直接打开原始URL。

## 可重建内容

**IndexReport**：`indexed_files,unchanged_files,removed_files,skipped_files,failed_files,details,status`。details最多100条诊断，计数覆盖全部实际访问文件；忽略规则排除的文件不计入。单文件请求也遵守工作区忽略规则。遍历或SQLite失败时回滚整批，单个文件不可读或无效时报告问题并移除旧索引。指纹包含解析器修订号和SHA-256。重新定位工作区会清除索引；索引可重建，不进入个人备份。

注释搜索使用同样的Unicode小写子串AND匹配和高亮片段，独立分页，按索引时间、工作区、路径和行号稳定排序。标签/收藏/示例过滤只用于已保存资料；全部来源搜索使用同一查询与范围。`indexed_at` 是最近解析时间，不保证磁盘内容仍然一致。`comments_target` 在跳转前重新核对指纹。支持语言与排除规则见[索引说明](indexing.md)。

Markdown按需预览显式保存的本地 `.md` / `.markdown`，要求UTF-8且不超过2MiB。核心渲染格式、转义原始HTML、移除链接/图片包装但保留文字/替代文本；适配器只读展示，不允许活动链接或资源加载。正文与HTML都不保存、不搜索；远程文档被拒绝，记录版本必须匹配。

共享版本和导入时间戳必须是JSON可精确表示的整数，最大9,007,199,254,740,991。超出范围直接拒绝，不能由JavaScript取整后继续导入。

## 备份预览校验

`backup_preview` 接收原始JSON条目，同时报告结构错误和业务错误。`invalid_count` 统计所有无效工作区/记录/关联条目（每个已解析条目最多一条语义错误），并计入备份级格式/时间戳问题。`invalid_entries` 最多100条，保留原始数组的0起始位置。JSON语法或顶层响应结构无效时，因无法确定条目数量，仍返回错误响应。

`invalid_count > 0` 时不生成导入计划，新增/未变/冲突字段只是空占位。两端应展示错误数量并停止确认流程，修正备份后重新预览。有效备份显示正常计划和冲突数量。`backup_import` 独立重做校验，任一错误都会拒绝整份备份，不能绕过预览检查直接强制导入。

## 飞书桌面跳转

`document_open_targets` 复用文档类型、版本、URL校验，原样返回 `original_url`，另提供可为空的 `feishu_applink`。符合条件的链接为使用默认端口的HTTPS、以 `.feishu.cn` 结尾的租户主机，以及支持的云文档路径。其他有效文档返回null；无效旧地址仍拒绝，过期版本仍返回CONFLICT。

生成URI为 `feishu://applink.feishu.cn/client/web_url/open?mode=window&url=<编码后的原地址>`。面向移动端的 `docs/open` 不支持PC。此操作不改写已保存地址；输入不接受任意feishu协议，适配器也不能信任Webview传入的打开目标。点击时重新通过核心解析选中的记录。见[飞书打开说明](feishu-opening.md)。

## 项目共享

每个项目拥有 `.codemori/shared.json`，读取时构建临时内存Store，不导入个人SQLite。最初版本使用格式1，当前写入格式3并兼容读取1和2。CLI不执行Git。

| 操作 | 请求 | 结果 |
|---|---|---|
| project_info | root | ProjectInfo；只读，在个人库创建前也可调用 |
| project_save | root,expected_version,record；可选配对id/revision和binding_path | 带项目元数据的Record，记录与可选关联原子保存 |
| project | root,request；写入必须有expected_version，读取可选 | 包装操作的结果，记录带项目元数据 |
| library_search | 可选root、scope=all/personal/project（默认all）、filter | 搜索页与可为空的项目状态，记录带scope |
| library_tags | 可选root、scope、demo=false | {tags,project} |
| library_file_documents | root,workspace_id,path | {records,entries,project}；个人与共享关联 |

**ProjectInfo**：`root,file,exists,version,error`。version是原始文件字节的SHA-256，未创建时为missing，无效数据时为unavailable。error是诊断，不代表可以覆盖。项目共享出错时个人结果仍可用。

共享记录派生 `scope:"project",project_root,project_version`，个人库记录带 `scope:"personal"`。旧式直接记录RPC保持原结构。UI复合身份是 `[scope,project_root,id]`，不能只按ID区分。临时工作区ID `project` 只在项目请求/视图内有效。选中记录后的操作，包括读取，必须携带当时的root和文件版本，防止Git变化后仍用旧选中项打开或复制内容。

已配置共享署名，且 `project_info` 返回 `version:"missing"` 后，可创建：

```json
{"protocol_version":1,"request":{"op":"project_save","root":"/work/team-project","expected_version":"missing","record":{"kind":"document","title":"重试规则","url":"./docs/retry.md","description":"仅超时重试"},"binding_path":"src/PaymentService.java"}}
```

共享文件顶层为 `format_version,records,bindings`。记录包含ID、版本、时间戳、input及后续格式引入的署名，关联包含path、document_id及可选kind/review。input包含kind/title/content/language/description/tags/source/url；source可为空，非空时为 `{path,line}`。不序列化个人工作区ID、绝对根目录、scope派生字段、starred或demo。

本地文档存为 `./相对路径`，打开时相对当前克隆解析；项目内绝对地址在保存时规范化。共享Obsidian允许vault和相对file，拒绝绝对path。读取时对全部字段和引用执行备份图校验。

项目包装允许record_create/update/delete/get、search、tags、document_link/unlink、binding_move、file_documents、document_bindings、source_target、document_target、document_open_targets、markdown_preview，以及下文新增的复核与修复操作。不允许嵌套project、个人备份、工作区、示例、收藏和索引写入。共享搜索清除workspace过滤，因为资料都属于当前项目，包括没有source的片段。跨库结果通过有界分页合并核心排名/更新时间/ID，完全相同时个人优先；收藏/示例筛选自然不返回共享结果。

写入先获取 `.shared.lock`（最多5秒），重新加载校验文件，核对读取时的文件哈希和记录revision，在原子替换前再次核对文件版本，然后以同目录同步临时文件替换。`.codemori/.gitignore` 忽略锁和临时文件。这只能串行化CodeMori写入；Git或任意外部编辑器不参与锁，保存期间应避免同时修改文件。它不是分布式事务或自动合并服务。

过期哈希/版本返回CONFLICT；锁超时返回STORAGE_ERROR；无效JSON返回INVALID_JSON；不支持的格式、图/路径或包装操作返回VALIDATION_ERROR；文件系统失败返回IO_ERROR。落盘前不能返回共享成功。仅共享搜索缺少项目根目录时校验失败；共享配置和本地文档路径拒绝符号链接。无效或冲突文件保持原字节，失败时适配器保留草稿。

## 代码链接与署名

共享格式2引入可为空的 `created_by` 和 `updated_by`，类型为Author `{id:UUID,display_name:string}`。新记录包含两者；编辑保留创建者，更新修改者。旧记录的创建者保持null，不相关记录的元数据保持不变，单独修改关联不改变正文署名。不支持该格式的旧核心会拒绝读取，避免丢失元数据；当前格式3继续保留这些字段。

全部项目写入需要本机署名配置；缺失时在获取共享锁或写入前返回IDENTITY_REQUIRED。操作者由核心从当前个人资料目录读取，不能由Webview或请求指定。署名是自报信息，不是身份认证。显示名去除首尾空白，长度1到80个Unicode标量字符，不含控制字符；修改显示名保留UUID。署名配置独立于个人记录备份。

| 操作 | 请求 | 响应 |
|---|---|---|
| identity_get | 无 | {author:Author或null}；不创建个人库 |
| identity_set | display_name | {author:Author}；带锁原子写入格式1的profile.json |
| code_link_create | root,path,line；可选jetbrains_product=idea、vscode_scheme=vscode | {vscode_url,jetbrains_url,project_id,path,line,created_project_identity} |
| code_link_parse | url | {project_id,path,line}及可选锚点；不写文件 |
| code_link_resolve | root,url | {target:目标对象或null}；项目标识不匹配时为null |

以上为顶层RPC，不能放进project包装。署名修改只创建profile/lock；生成代码链接可创建project.json和锁忽略规则，不创建个人SQLite或shared.json。项目标识格式1为 `{format_version:1,project_id:UUID}`，读取和解析不会创建。配置、标识和锁路径都需防止符号链接；已有无效标识文件保持原样。

URI格式为 `vscode://lusyne.codemori/open?project=<uuid>&path_hex=<UTF8十六进制>&line=<1起始行号>` 和 `jetbrains://<产品>/codemori/open?...`，VSCode Insiders使用vscode-insiders。允许的JetBrains前缀为idea/pycharm/webstorm/goland/clion/rider/rubymine/phpstorm/rustrover/datagrip，兼容性仍以实际产品验收为准。

拒绝其他scheme/authority/action、凭据、端口、fragment、重复或未知参数、无效UUID、错误十六进制/UTF-8、目录越界和控制字符。行号1到2147483647，URI最多8KiB。path_hex使用ASCII十六进制，避免VSCode Uri.parse解码查询参数后混淆空格和字面量加号；这是编码，不是加密。

解析只使用适配器提供的已打开或显式选择的本地项目根目录。匹配项目标识后，才能解析不含符号链接的普通文件。不得执行命令、信任URI中的绝对根路径或扫描磁盘。VSCode未匹配时要求选目录；JetBrains要求先打开正确项目再粘贴。多个打开的克隆需要用户选择。目标包含root、本地绝对path、line和project_id；锚点解析信息见下文。

可选值必须留在响应对象内，如 `{author:null}`、`{target:null}`，不能让顶层data为null，两端都会拒绝。首次署名与项目不匹配应通过实际客户端验证。

## 上下文知识

schema4为关联增加kind/review字段。从schema3升级先做在线备份，再事务迁移，旧核心拒绝schema4。备份导出2/读取1和2，共享读取1到3/写入3。旧关联默认file且未确认，已有元数据保留；项目标识与署名配置仍为格式1。

Binding增加可选 `kind:"file"|"module"`（默认file），以及可为空的 `review:{fingerprint,confirmed_at,confirmed_by,git_commit,path}`。fingerprint是规范化后已保存文本的SHA256，不保存源码正文快照。模块路径 `.` 表示项目根目录，其他路径使用严格相对路径规则；继承匹配要求斜杠边界。备份重复与冲突按workspace/path/document身份判断，导入时保留不同的本地元数据。

目标可读时，`document_link` 和带binding_path的 `project_save` 建立基线；project_save的binding_kind默认file。重复关联复用已有基线。文件匹配与搜索包含模块文档；`library_file_documents` 在去重records之外返回 `entries:[{record,binding,inherited,review_state}]`。

ReviewState字段为status、fingerprint、error；status取current、needs_review、unconfirmed或unavailable，后两项可为null。不可检查不能当作current。同一目标的多篇文档复用本次指纹，但分别比较各自基线。

| 操作 | 请求 | 结果 |
|---|---|---|
| binding_changes | binding | {binding,review_state,diff:字符串或null,note}；可选只读Git差异 |
| binding_review | binding（完整观察值）,fingerprint（当前代码观察值）,document_revision | 更新后的binding；确认前核对三者 |
| paths_repair | workspace_id,from,to，可选token | {token,bindings,records,applied,from,to,record_ids}；无token为预览，匹配token时执行 |

三项都支持project包装。共享写入保留文件版本比较并要求署名；个人确认在已配置署名时使用该身份。修复拒绝目标冲突、校验全部文件/目录，以受影响关联和记录快照的哈希作为token；单库事务应用，共享文件再原子替换。修复片段source会递增记录revision并更新共享修改者；它不移动磁盘文件，也不编辑外部文档或URI字符串。

复核每个UTF-8文件最多2MiB，CRLF归一化为LF。模块遍历遵守忽略规则，跳过隐藏/构建/符号链接，最多2000个非忽略文件与64MiB受支持源码。扩展名见[上下文说明](contextual-knowledge.md)。共享文件和RPC均有128MiB上限。

可选Git status/ls-files/rev-parse/diff不使用shell、网络、外部diff/textconv/fsmonitor，最多5秒和1MiB标准输出。只有干净且已提交的基线可用，忽略/未跟踪文件没有Git基线。缺少Git/历史或超限时返回diff:null和说明，指纹复核仍可使用。

代码链接可带 `anchor_hex`，内容为ASCII十六进制JSON锚点 `{file_hash,line_hash:null或字符串,symbol:null或字符串}`。校验哈希，symbol最多4096字节，总URI仍最多8KiB。Java/JS/TS/Python符号使用限定声明路径/类型签名，深度最多256。文件内容不变时保持原行；唯一符号或行内容可重新定位；符号变化时可能定位声明起始行。路径丢失时进行有界唯一文件/符号查找，有歧义则拒绝，不选择第一个结果。不带anchor_hex的旧链接保持旧行号语义。新目标返回 `resolution:exact|symbol|content|relocated|legacy_line`。

## 批量复核与AI证据

schema4、备份2、共享3与协议1保持不变。`bindings_review {items:[{binding,fingerprint,document_revision}]}` 接受1到200个不同关联。先核对全部观察值、当前代码指纹和文档revision，再用一个事务提交；任一冲突或失败都不修改该库整批数据。批内复用目标读取。project包装将工作区ID映射为project，一次文件版本比较与原子替换保存全部结果。返回 `{confirmed,bindings}`，共享结果另带project_version。不自动判断空白改动是否语义等价。

| 操作 | 请求 | 结果 |
|---|---|---|
| code_search | root,query,under?,limit? | hits[{path,line,excerpt,excerpt_truncated}],scanned_files,skipped_files,truncated,diagnostics,scope,under |
| code_read | root,path,start_line?,end_line? | path,start_line,end_line,total_lines,content,fingerprint,truncated,scope |
| ai_context | root,query?,path? | root,scope=current_project,knowledge,associations?,code?,external_document_bodies_loaded=false,review_confirmation_performed=false |

源码操作仅允许顶层调用，使用安全相对路径，拒绝符号链接越界，读取UTF-8。搜索按行进行忽略大小写的字面量AND匹配：query为1到1024字节，limit为1到50（默认20），单文件2MiB，最多遍历10000项/2000文件/64MiB累计体积，片段最多300字符。复用核心忽略规则，明确报告截断和跳过。

按行读取从1开始，最多200行/64KiB，起始行超出文件则失败。code_search/code_read不创建个人库。ai_context登记项目映射，将个人知识限制到该工作区，合并当前共享资料，可附文件/模块关联；不修改记录、复核基线或源码，也不读取远程正文。序列化上下文最多512KiB，超过时返回校验错误，应缩小查询或使用code_search/code_read，不能静默丢弃部分结果。

两端批量确认只覆盖当前文件关联区显示的needs_review项（含继承模块），使用显示时捕获的元数据，按个人/共享分组。每个库原子更新，不提供跨库事务，部分成功必须准确显示数量与失败原因。点击标明范围的按钮即确认，不再二次弹窗；已有的首次共享署名设置除外。
