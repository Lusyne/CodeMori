import { openFeishuAppLink } from './feishu';
import * as vscode from 'vscode';
import * as fs from 'node:fs/promises';
import * as path from 'node:path';
import * as os from 'node:os';
import { randomUUID } from 'node:crypto';
import { CoreClient, bundledExecutable } from './core';
import * as p from './protocol';
import { html, markdownHtml, snippetHtml } from './webview';
import { DocumentHints } from './hints';

interface FileContext { root?: string; path?: string; line?: number; code: string; language: string; column?: vscode.ViewColumn }
interface ResolvedContext extends FileContext { workspaceId?: string }
interface Draft { token: string; input: p.RecordInput; existing?: p.KnowledgeRecord; binding?: p.Binding; project?: p.ProjectInfo | null }
interface RenderMeasurement { elapsedMs: number; total: number; rows: number; visible: boolean }

class CodeMori implements vscode.Disposable, vscode.UriHandler, vscode.WebviewViewProvider {
    private view?: vscode.WebviewView;
    private ready = false;
    private queued: unknown[] = [];
    private lastEditor?: vscode.TextEditor;
    private draft?: Draft;
    private readonly subscriptions: vscode.Disposable[] = [];
    private readonly client: CoreClient;
    private readonly hints: DocumentHints;
    private testReady = false;
    private receivedCodeLinks = 0;
    private renderedProjectVersion: string | null = null;
    private readonly projectRoots = new Set<string>();
    private readonly reviewContexts = new Map<string, {label: string; entries: p.Association[]}>();
    private reviewing = false;
    private sharedRefresh?: ReturnType<typeof setTimeout>;
    private readonly measurements = new Map<string, { resolve: (value: RenderMeasurement) => void; reject: (error: Error) => void }>();

    constructor(private readonly context: vscode.ExtensionContext) {
        const isolated = context.extensionMode === vscode.ExtensionMode.Production ? undefined : process.env.CODEMORI_TEST_DATA_DIR;
        this.client = new CoreClient(bundledExecutable(context.extensionPath), isolated);
        this.hints = new DocumentHints(this.client);
        this.subscriptions.push(this.hints, vscode.languages.registerCodeLensProvider({ scheme: 'file' }, this.hints),
            vscode.workspace.onDidChangeConfiguration(e => { if (e.affectsConfiguration('codemori.documentHints.enabled')) this.hints.invalidate(); }),
            vscode.workspace.onDidSaveTextDocument(() => { this.hints.invalidate(); this.post({ type: 'contextChanged' }); }),
            vscode.workspace.onDidRenameFiles(() => { this.hints.invalidate(); this.post({ type: 'contextChanged' }); }));
        this.lastEditor = vscode.window.activeTextEditor;
        this.subscriptions.push(vscode.window.onDidChangeActiveTextEditor(editor => {
            if (editor) { this.lastEditor = editor; this.hints.invalidate(); this.post({ type: 'contextChanged' }); }
        }), vscode.workspace.onDidCloseTextDocument(document => {
            if (this.lastEditor?.document === document) { this.lastEditor = undefined; this.post({ type: 'contextChanged' }); }
        }));
        const watcher = vscode.workspace.createFileSystemWatcher('**/.codemori/shared.json');
        const changed = () => {
            if (this.sharedRefresh) clearTimeout(this.sharedRefresh);
            this.sharedRefresh = setTimeout(() => { this.hints.invalidate(); this.post({ type: 'contextChanged' }); }, 100);
        };
        this.subscriptions.push(watcher, watcher.onDidChange(changed), watcher.onDidCreate(changed), watcher.onDidDelete(changed));
    }
    dispose(): void { if (this.sharedRefresh) clearTimeout(this.sharedRefresh); this.subscriptions.forEach(item => item.dispose()); }
    async open(): Promise<void> {
        await vscode.commands.executeCommand('codemori.library.focus');
        const view = this.view;
        if (!view) throw new p.CoreError('OPEN_FAILED', '无法打开 CodeMori 侧边栏，请在活动栏中重新打开。');
        // Revealing a retained view is asynchronous; do not send a draft while it is hidden.
        if (!view.visible) await new Promise<void>((resolve, reject) => {
            const timer = setTimeout(() => { listener.dispose(); reject(new p.CoreError('OPEN_FAILED', 'CodeMori 侧边栏未显示，请重试。')); }, 5000);
            const listener = view.onDidChangeVisibility(() => {
                if (view.visible) { clearTimeout(timer); listener.dispose(); resolve(); }
            });
        });
    }
    resolveWebviewView(view: vscode.WebviewView): void {
        this.ready = false;
        this.testReady = false;
        this.view = view;
        view.webview.options = { enableScripts: true,
            localResourceRoots: [vscode.Uri.joinPath(this.context.extensionUri, 'media')] };
        this.subscriptions.push(view.onDidDispose(() => { if (this.view === view) {
            this.view = undefined; this.ready = false; this.testReady = false; this.queued = [];
            for (const pending of this.measurements.values()) pending.reject(new Error('Sidebar closed during measurement'));
        } }), view.onDidChangeVisibility(() => {
            if (view.visible && this.view === view) this.post({ type: 'contextChanged' });
        }), view.webview.onDidReceiveMessage((value: unknown) => { void this.handle(value, view); }));
        view.webview.html = html(view.webview, this.context.extensionUri);
    }
    private post(value: unknown): void {
        if (!this.view) return;
        if (!this.ready) this.queued.push(value);
        else void this.view.webview.postMessage(value);
    }
    private capture(): FileContext {
        const editor = vscode.window.activeTextEditor ?? this.lastEditor;
        const folders = vscode.workspace.workspaceFolders;
        const fallback = folders?.length === 1 && folders[0].uri.scheme === 'file' ? folders[0].uri.fsPath : undefined;
        if (!editor || editor.document.isClosed) return { root: fallback, code: '', language: '' };
        if (!['file', 'untitled'].includes(editor.document.uri.scheme)) return { code: '', language: '' };
        const folder = vscode.workspace.getWorkspaceFolder(editor.document.uri);
        const root = folder?.uri.scheme === 'file' ? folder.uri.fsPath : fallback;
        const relative = root && editor.document.uri.scheme === 'file' ? path.relative(root, editor.document.uri.fsPath) : undefined;
        const contained = relative && !relative.startsWith(`..${path.sep}`) && relative !== '..' && !path.isAbsolute(relative);
        return { root, path: contained ? relative.split(path.sep).join('/') : undefined,
            line: editor.selection.start.line + 1, code: editor.document.getText(editor.selection),
            language: editor.document.languageId, column: editor.viewColumn };
    }
    private async resolve(context: FileContext): Promise<ResolvedContext> {
        if (!context.root) return context;
        try { if (!(await fs.stat(context.root)).isDirectory()) return context; }
        catch (error) { if ((error as NodeJS.ErrnoException).code === 'ENOENT') return context; throw error; }
        const workspace = p.workspace(await this.client.call({ op: 'workspace_register', root: context.root }));
        this.projectRoots.add(workspace.root);
        return { ...context, root: workspace.root, workspaceId: workspace.id };
    }
    private route(message: Record<string, unknown>): p.ProjectRef | undefined {
        if (p.recordScope(message.scope) === 'personal') return undefined;
        const root = p.string(message.projectRoot); const version = p.string(message.projectVersion);
        if (!this.projectRoots.has(root)) throw new p.CoreError('INVALID_REQUEST', '项目资料只能使用编辑器已打开的项目范围。');
        return { root, version };
    }
    private async call(request: Record<string, unknown>, route?: p.ProjectRef): Promise<unknown> {
        if (route && (request.op !== 'paths_repair' || typeof request.token === 'string') && ['record_create', 'record_update', 'record_delete', 'document_link', 'document_unlink', 'binding_move', 'binding_review', 'bindings_review', 'paths_repair'].includes(String(request.op))) await this.ensureIdentity();
        return this.client.call(route ? { op: 'project', root: route.root, expected_version: route.version, request } : request);
    }
    private async projectContext(root?: string): Promise<p.ProjectInfo | null> {
        if (!root) return null;
        const project = p.projectInfo(await this.client.call({ op: 'project_info', root }));
        if (project && !project.error) this.projectRoots.add(project.root);
        return project;
    }
    private async handle(value: unknown, sender: vscode.WebviewView): Promise<{ok: boolean; data?: unknown; error?: string} | undefined> {
        if (this.view !== sender) return;
        let id: string | undefined;
        try {
            const message = p.object(value); id = p.string(message.id); const action = p.string(message.action);
            const route = ['edit', 'favorite', 'delete', 'copy', 'open', 'documentTargets', 'openFeishu', 'markdown', 'previewSnippet', 'bind', 'bindModule', 'unlink', 'repairBinding', 'review', 'changes'].includes(action) ? this.route(message) : undefined;
            let data: unknown;
            switch (action) {
                case 'ready':
                    this.ready = true;
                    for (const pending of this.queued.splice(0)) this.post(pending);
                    data = { onboardingDismissed: this.context.globalState.get('onboardingDismissed', false), testMode: this.context.extensionMode === vscode.ExtensionMode.Test };
                    break;
                case 'testReady':
                    if (this.context.extensionMode !== vscode.ExtensionMode.Test) throw new Error('Test interface unavailable');
                    this.testReady = true; data = {}; break;
                case 'testProjectRendered':
                    if (this.context.extensionMode !== vscode.ExtensionMode.Test) throw new Error('Test interface unavailable');
                    this.renderedProjectVersion = message.version === null ? null : p.string(message.version); data = {}; break;
                case 'testMeasured': {
                    if (this.context.extensionMode !== vscode.ExtensionMode.Test) throw new Error('Test interface unavailable');
                    const pending = this.measurements.get(p.string(message.token));
                    if (!pending) throw new Error('Unknown measurement');
                    if (message.error) pending.reject(new Error(p.string(message.error)));
                    else {
                        if (typeof message.elapsedMs !== 'number' || !Number.isFinite(message.elapsedMs)) throw new Error('Invalid elapsed time');
                        pending.resolve({ elapsedMs: message.elapsedMs, total: p.number(message.total), rows: p.number(message.rows), visible: p.boolean(message.visible) });
                    }
                    data = {}; break;
                }
                case 'search': data = await this.search(message); break;
                case 'reviewCurrent': data = await this.reviewCurrent(p.string(message.token)); break;
                case 'toggleHints': await this.toggleHints(); data = {}; break;
                case 'repairPaths': await this.repairPaths(); data = {}; break;
                case 'review': data = await this.confirmReview(p.binding(message.binding), p.number(message.revision), p.string(message.fingerprint), route); break;
                case 'changes': await this.showChanges(p.binding(message.binding), route); data = {}; break;
                case 'bindModule': data = await this.bind(p.string(message.recordId), p.number(message.revision), route, 'module'); break;
                case 'copyCodeLink': await this.copyCodeLink(); data = {}; break;
                case 'openCodeLink': await this.pasteCodeLink(); data = {}; break;
                case 'identity': await this.configureIdentity(); data = {}; break;
                case 'capture': await this.saveSelection(); data = {}; break;
                case 'newSnippet': await this.newSnippet(); data = {}; break;
                case 'associate': await this.associateDocument(); data = {}; break;
                case 'edit': await this.edit(p.string(message.recordId), route); data = {}; break;
                case 'save': data = await this.saveDraft(message); break;
                case 'discard': this.draft = undefined; data = {}; break;
                case 'favorite': data = await this.favorite(p.string(message.recordId), p.number(message.revision), route); break;
                case 'delete': data = await this.remove(p.string(message.recordId), p.number(message.revision), route); break;
                case 'copy': data = await this.copy(p.string(message.recordId), p.number(message.revision), route); break;
                case 'open': await this.openRecord(p.string(message.recordId), p.number(message.revision), route); data = {}; break;
                case 'documentTargets': data = await this.documentTargets(p.string(message.recordId), p.number(message.revision), route); break;
                case 'openFeishu': await this.openFeishu(p.string(message.recordId), p.number(message.revision), route); data = {}; break;
                case 'previewSnippet': await this.previewSnippet(p.string(message.recordId), p.number(message.revision), route); data = {}; break;
                case 'markdown': await this.previewMarkdown(p.string(message.recordId), p.number(message.revision), route); data = {}; break;
                case 'indexFile': data = await this.indexComments(true); break;
                case 'indexWorkspace': data = await this.indexComments(false); break;
                case 'openComment': {
                    const source = p.source(message.source);
                    const target = p.string(await this.client.call({ op: 'comments_target', source }));
                    await this.openFile(vscode.Uri.file(target), source.line ?? undefined); data = {}; break;
                }
                case 'bind': data = await this.bind(p.string(message.recordId), p.number(message.revision), route); break;
                case 'unlink': data = await this.unlink(p.binding(message.binding), route); break;
                case 'repairBinding': await this.repairBinding(p.string(message.recordId), route); data = {}; break;
                case 'relocate': await this.relocate(); data = {}; break;
                case 'export': data = await this.exportBackup(); break;
                case 'import': data = await this.importBackup(); break;
                case 'dismissOnboarding': await this.context.globalState.update('onboardingDismissed', true); data = {}; break;
                default: throw new p.CoreError('INVALID_REQUEST', 'Unsupported interface action.');
            }
            if (['save', 'bind', 'bindModule', 'unlink', 'review', 'repairBinding', 'delete'].includes(action)) this.hints.invalidate();
            if (this.view === sender) this.post({ type: 'reply', id, ok: true, data });
            return { ok: true, data };
        } catch (error) {
            if (this.view === sender) this.post({ type: 'reply', id, ok: false, error: p.messageError(error) });
            return { ok: false, error: p.messageError(error) };
        }
    }
    testInterface() {
        const waitForRender = async () => {
            if (this.context.extensionMode !== vscode.ExtensionMode.Test) throw new Error('Test interface unavailable');
            const deadline = Date.now() + 15_000;
            while (!this.testReady && Date.now() < deadline) await new Promise(resolve => setTimeout(resolve, 25));
            if (!this.testReady) throw new Error('Webview was not ready for measurement');
        };
        return {
            waitForRender,
            measureSearch: async (query: string): Promise<RenderMeasurement> => {
                this.view?.show(true);
                await waitForRender();
                const token = randomUUID(); let timer: NodeJS.Timeout | undefined;
                try {
                    return await new Promise<RenderMeasurement>((resolve, reject) => {
                        this.measurements.set(token, { resolve, reject });
                        timer = setTimeout(() => reject(new Error('Webview render measurement timed out')), 15_000);
                        this.post({ type: 'testSearch', token, query });
                    });
                } finally { if (timer) clearTimeout(timer); this.measurements.delete(token); }
            },
            sidebar: () => ({ visible: this.view?.visible ?? false, viewType: this.view?.viewType }),
            projectVersion: () => this.renderedProjectVersion,
            receivedCodeLinks: () => this.receivedCodeLinks,
            receiveCodeLink: (url: string) => this.navigateCodeLink(url),
            draft: () => this.draft ? structuredClone(this.draft) : undefined,
            request: (value: unknown) => {
                if (!this.view) throw new Error('Open the panel before an integration request.');
                return this.handle(value, this.view);
            }
        };
    }
    private async search(message: Record<string, unknown>): Promise<unknown> {
        const filter = p.uiFilter(message.filter); const context = await this.resolve(this.capture());
        const libraryScope = p.libraryScope(message.libraryScope);
        if (p.boolean(message.projectOnly ?? false)) {
            if (!context.workspaceId) throw new p.CoreError('NO_WORKSPACE', '当前本地工作区不可用。可使用全部资料，或重新定位工作区。');
            filter.workspace_id = context.workspaceId;
        }
        // Refreshing tags after mutations must update the actual filter as well as the visible selector.
        let tags: string[] | undefined;
        if (p.boolean(message.reloadTags ?? false)) {
            const data = p.object(await this.client.call({ op: 'library_tags', root: context.root, scope: libraryScope, demo: filter.demo }));
            tags = p.array(data.tags, p.string);
            if (filter.tag && !tags.some(tag => tag.toLowerCase() === filter.tag!.toLowerCase())) delete filter.tag;
        }
        const saved = filter.kind === 'comment' ? { items: [], total: 0, limit: filter.limit, offset: filter.offset, project: null }
            : p.object(await this.client.call({ op: 'library_search', root: context.root, scope: libraryScope, filter }));
        const result = p.page(saved);
        let project = p.projectInfo(saved.project);
        const comments = (!filter.kind || filter.kind === 'comment') && !filter.tag && !filter.starred && !filter.demo
            ? p.commentPage(await this.client.call({ op: 'comments_search', filter: { query: filter.query, workspace_id: filter.workspace_id, offset: filter.offset, limit: filter.limit } }))
            : { items: [], total: 0, limit: filter.limit, offset: filter.offset };
        const indexStatus = context.workspaceId ? p.indexStatus(await this.client.call({ op: 'comments_status', workspace_id: context.workspaceId })) : null;
        let documents: p.KnowledgeRecord[] = []; let associations: p.Association[] = [];
        if (context.workspaceId && context.path) {
            const links = p.object(await this.client.call({ op: 'library_file_documents', root: context.root, workspace_id: context.workspaceId, path: context.path }));
            associations = p.array(links.entries, p.association); documents = p.array(links.records, p.record); project = p.projectInfo(links.project);
        }
        const reviewToken = randomUUID();
        if (this.reviewContexts.size >= 8) this.reviewContexts.delete(this.reviewContexts.keys().next().value!);
        this.reviewContexts.set(reviewToken, {label: `${path.basename(context.root || "")}/${context.path || ""}`, entries: associations});
        return { reviewToken, result, comments, indexStatus, documents, associations, tags, filter, project, libraryScope, file: context.path ?? null, workspaceId: context.workspaceId ?? null };
    }
    private async indexComments(singleFile: boolean): Promise<unknown> {
        const context = await this.resolve(this.capture());
        if (!context.workspaceId || (singleFile && !context.path)) throw new p.CoreError('NO_FILE', '请先打开本地项目中的源码文件。');
        return vscode.window.withProgress({ location: vscode.ProgressLocation.Notification, title: 'CodeMori：索引已保存的源码注释', cancellable: true },
            async (_progress, cancellation) => {
                const controller = new AbortController();
                const listener = cancellation.onCancellationRequested(() => controller.abort());
                try { return p.indexReport(await this.client.call({ op: 'comments_index', workspace_id: context.workspaceId, path: singleFile ? context.path : null }, controller.signal)); }
                finally { listener.dispose(); }
            });
    }
    async saveSelection(): Promise<void> {
        const snapshot = this.capture();
        if (!snapshot.code.trim()) { void vscode.window.showInformationMessage('请先选中代码，或在 CodeMori 中新建片段。'); return; }
        const source = await this.resolve(snapshot);
        const input = p.input({ kind: 'snippet', content: snapshot.code, language: snapshot.language,
            source: source.workspaceId && source.path ? { workspace_id: source.workspaceId, path: source.path, line: source.line } : null });
        await this.editor({ token: randomUUID(), input, project: await this.projectContext(source.root) });
    }
    private async newSnippet(): Promise<void> {
        const context = await this.resolve(this.capture());
        await this.editor({ token: randomUUID(), input: p.input({ kind: 'snippet' }), project: await this.projectContext(context.root) });
    }
    async associateDocument(): Promise<void> {
        const source = await this.resolve(this.capture());
        if (!source.workspaceId || !source.path) { void vscode.window.showInformationMessage('请先打开本地项目中的源码文件。'); return; }
        await this.editor({ token: randomUUID(), input: p.input({ kind: 'document' }),
            binding: { workspace_id: source.workspaceId, path: source.path, document_id: '' }, project: await this.projectContext(source.root) });
    }
    private async edit(id: string, route?: p.ProjectRef): Promise<void> {
        const existing = p.record(await this.call({ op: 'record_get', id }, route));
        await this.editor({ token: randomUUID(), input: existing.input, existing,
            project: route ? { ...route, exists: true, file: path.join(route.root, '.codemori/shared.json'), error: null } : null });
    }
    private async editor(draft: Draft): Promise<void> {
        if (this.draft && await vscode.window.showWarningMessage('已有未保存的草稿，要替换吗？', { modal: true }, '替换') !== '替换') return;
        this.draft = draft; await this.open();
        const tagData = p.object(await this.client.call({ op: 'library_tags', root: draft.project?.root, scope: draft.existing?.scope ?? 'all', demo: draft.existing?.is_demo ?? false }));
        const tags = p.array(tagData.tags, p.string);
        if (this.draft !== draft) return;
        this.post({ type: 'edit', token: draft.token, input: draft.input, tags, scope: draft.existing?.scope ?? 'personal', scopeLocked: Boolean(draft.existing), project: draft.project,
            title: draft.existing ? '编辑资料' : draft.input.kind === 'snippet' ? '保存代码片段' : '关联文档', binding: draft.binding });
    }
    private async saveDraft(message: Record<string, unknown>): Promise<p.KnowledgeRecord> {
        const draft = this.draft;
        if (!draft || p.string(message.token) !== draft.token) throw new p.CoreError('DRAFT_EXPIRED', '草稿已过期，请复制输入后重新打开编辑。');
        const edited = p.input(message.input);
        if (edited.kind !== draft.input.kind) throw new p.CoreError('INVALID_REQUEST', '不能更改资料类型。');
        // The form can repair a relative source path, but cannot retarget an old capture to a new workspace.
        edited.source = draft.input.source ? { ...draft.input.source, path: edited.source?.path ?? draft.input.source.path } : null;
        const scope = p.recordScope(message.scope ?? draft.existing?.scope);
        if (draft.existing && scope !== draft.existing.scope) throw new p.CoreError('INVALID_REQUEST', '编辑已有资料不能改变保存范围。');
        const targetKind = p.bindingKind(message.bindingKind);
        const targetBinding = draft.binding ? { ...draft.binding, kind: targetKind, path: targetKind === 'module' ? path.posix.dirname(draft.binding.path) : draft.binding.path } : undefined;
        let saved: p.KnowledgeRecord;
        if (scope === 'project') {
            if (!draft.project || draft.project.error) throw new p.CoreError('PROJECT_UNAVAILABLE', draft.project?.error || '请先打开本地项目。');
            await this.ensureIdentity();
            saved = p.record(await this.client.call({ op: 'project_save', root: draft.project.root,
                expected_version: draft.project.version, id: draft.existing?.id, revision: draft.existing?.revision,
                record: edited, binding_path: targetBinding?.path, binding_kind: targetKind }));
        } else {
            const request = draft.existing ? { op: 'record_update', id: draft.existing.id, revision: draft.existing.revision, record: edited }
                : { op: 'record_create', record: edited };
            saved = p.record(await this.client.call(request));
            if (targetBinding) await this.client.call({ op: 'document_link', binding: { ...targetBinding, document_id: saved.id } });
        }
        if (this.draft === draft) this.draft = undefined;
        return saved;
    }
    private async observed(id: string, revision: number, route?: p.ProjectRef): Promise<p.KnowledgeRecord> {
        const record = p.record(await this.call({ op: 'record_get', id }, route));
        if (record.revision !== revision) throw new p.CoreError('CONFLICT', 'Record changed; refresh before acting.');
        return record;
    }
    private async favorite(id: string, revision: number, route?: p.ProjectRef): Promise<p.KnowledgeRecord> {
        const record = await this.observed(id, revision, route);
        if (route) throw new p.CoreError('INVALID_REQUEST', '收藏仅属于个人资料。');
        if (record.input.kind !== 'snippet') throw new p.CoreError('INVALID_REQUEST', '仅代码片段支持收藏。');
        return p.record(await this.client.call({ op: 'record_update', id, revision: record.revision, record: { ...record.input, starred: !record.input.starred } }));
    }
    private async remove(id: string, revision: number, route?: p.ProjectRef): Promise<unknown> {
        const record = await this.observed(id, revision, route);
        if (await vscode.window.showWarningMessage(`删除“${record.input.title}”？不会删除源文件或外部文档。`, { modal: true }, '删除') !== '删除') return { cancelled: true };
        return this.call({ op: 'record_delete', id, revision: record.revision }, route);
    }
    private async copy(id: string, revision: number, route?: p.ProjectRef): Promise<unknown> {
        const record = await this.observed(id, revision, route);
        await vscode.env.clipboard.writeText(record.input.kind === 'snippet' ? record.input.content : record.input.url!);
        return { copied: true };
    }
    private async documentTargets(id: string, revision: number, route?: p.ProjectRef): Promise<p.DocumentOpenTargets> {
        return p.documentOpenTargets(await this.call({ op: 'document_open_targets', id, revision }, route));
    }
    private async openFeishu(id: string, revision: number, route?: p.ProjectRef): Promise<void> {
        const target = (await this.documentTargets(id, revision, route)).feishu_applink;
        if (!target) throw new p.CoreError('INVALID_LINK', '此文档不支持飞书客户端打开，请使用原文入口。');
        await openFeishuAppLink(target);
    }
    private async openRecord(id: string, revision: number, route?: p.ProjectRef): Promise<void> {
        const record = await this.observed(id, revision, route);
        if (record.input.kind === 'document') {
            const uri = vscode.Uri.parse(p.string(await this.call({ op: 'document_target', id, revision }, route)));
            if (uri.scheme === 'file') await this.openFile(uri);
            else if (['http', 'https', 'obsidian'].includes(uri.scheme)) {
                if (!(await vscode.env.openExternal(uri))) throw new p.CoreError('OPEN_FAILED', '无法打开原文，请检查对应应用是否安装。');
            } else throw new p.CoreError('INVALID_LINK', '不支持的文档链接。');
        } else {
            if (!record.input.source) throw new p.CoreError('NO_SOURCE', '这个片段没有来源文件。');
            const target = p.string(await this.call({ op: 'source_target', source: record.input.source }, route));
            await this.openFile(vscode.Uri.file(target), record.input.source.line ?? undefined);
        }
    }
    private async previewSnippet(id: string, revision: number, route?: p.ProjectRef): Promise<void> {
        const record = await this.observed(id, revision, route);
        if (record.input.kind !== 'snippet') throw new p.CoreError('INVALID_REQUEST', '只有代码片段可以打开快照预览。');
        const panel = vscode.window.createWebviewPanel('codemori.snippet', `代码快照 · ${record.input.title}`, vscode.ViewColumn.Active,
            { enableScripts: false, localResourceRoots: [] });
        panel.webview.html = snippetHtml(record.input.title, record.input.content);
        this.context.subscriptions.push(panel);
    }
    private async previewMarkdown(id: string, revision: number, route?: p.ProjectRef): Promise<void> {
        const preview = p.markdownPreview(await this.call({ op: 'markdown_preview', id, revision }, route));
        const panel = vscode.window.createWebviewPanel('codemori.markdown', `只读预览 · ${preview.title}`, vscode.ViewColumn.Active,
            { enableScripts: false, localResourceRoots: [] });
        panel.webview.html = markdownHtml(preview);
        this.context.subscriptions.push(panel);
    }
    private async openFile(uri: vscode.Uri, line?: number): Promise<void> {
        const document = await vscode.workspace.openTextDocument(uri);
        const editor = await vscode.window.showTextDocument(document, this.lastEditor?.viewColumn ?? vscode.ViewColumn.One);
        if (line !== undefined) {
            const point = Math.min(Math.max(0, line - 1), document.lineCount - 1);
            editor.selection = new vscode.Selection(point, 0, point, 0);
            editor.revealRange(new vscode.Range(point, 0, point, 0), vscode.TextEditorRevealType.InCenterIfOutsideViewport);
        }
    }
    private async bind(id: string, revision: number, route?: p.ProjectRef, kind: 'file' | 'module' = 'file'): Promise<unknown> {
        await this.observed(id, revision, route);
        const context = await this.resolve(this.capture());
        if (!context.workspaceId || !context.path) throw new p.CoreError('NO_FILE', '请先打开本地项目中的源码文件。');
        if (route && route.root !== context.root) throw new p.CoreError('INVALID_REQUEST', '共享文档只能关联到同一项目的文件。');
        return this.call({ op: 'document_link', binding: { workspace_id: context.workspaceId, path: kind === 'module' ? path.posix.dirname(context.path) : context.path, document_id: id, kind } }, route);
    }
    private async unlink(binding: p.Binding, route?: p.ProjectRef): Promise<unknown> { return this.call({ op: 'document_unlink', binding }, route); }
    private async repairBinding(id: string, route?: p.ProjectRef): Promise<void> {
        const bindings = p.array(await this.call({ op: 'document_bindings', document_id: id }, route), p.binding);
        const workspaces = route ? [{ id: 'project', name: path.basename(route.root), root: route.root, revision: 1 }] : p.array(await this.client.call({ op: 'workspace_list' }), p.workspace);
        const options = bindings.map(binding => ({ label: binding.path,
            description: workspaces.find(w => w.id === binding.workspace_id)?.name,
            detail: workspaces.find(w => w.id === binding.workspace_id)?.root, binding }));
        if (!options.length) { void vscode.window.showInformationMessage('这篇文档尚未关联文件。'); return; }
        const selected = await vscode.window.showQuickPick(options, { title: '选择要修复的关联' });
        if (!selected) return;
        const target = await vscode.window.showInputBox({ title: '新的项目相对路径', value: selected.binding.path });
        if (target === undefined) return;
        await this.call({ op: 'binding_move', binding: selected.binding, path: target }, route);
    }
    private async relocate(): Promise<void> {
        const workspaces = p.array(await this.client.call({ op: 'workspace_list' }), p.workspace);
        const selected = await vscode.window.showQuickPick(workspaces.map(w => ({ label: w.name, description: w.root, workspace: w })), { title: '选择要重新定位的工作区' });
        if (!selected) return;
        const folders = await vscode.window.showOpenDialog({ canSelectFiles: false, canSelectFolders: true, canSelectMany: false, title: '选择新的根目录' });
        if (!folders?.length || folders[0].scheme !== 'file') return;
        await this.client.call({ op: 'workspace_relocate', id: selected.workspace.id, revision: selected.workspace.revision, root: folders[0].fsPath });
    }
    async toggleHints(): Promise<void> {
        const config = vscode.workspace.getConfiguration('codemori', vscode.window.activeTextEditor?.document.uri);
        await config.update('documentHints.enabled', !config.get('documentHints.enabled', true), vscode.workspace.workspaceFolders?.length ? vscode.ConfigurationTarget.Workspace : vscode.ConfigurationTarget.Global);
        this.hints.invalidate();
    }
    private async reviewCurrent(token: string): Promise<unknown> {
        if (this.reviewing) throw new Error('正在确认，请稍候。');
        const context = this.reviewContexts.get(token); if (!context) throw new Error('当前文件视图已过期，请刷新后重试。');
        this.reviewContexts.delete(token); this.reviewing = true;
        let confirmed = 0; const errors: string[] = [];
        try {
            const groups = new Map<string, p.Association[]>();
            for (const entry of context.entries.filter(e => e.review_state.status === 'needs_review' && e.review_state.fingerprint)) {
                const key = JSON.stringify([entry.record.scope, entry.record.project_root, entry.record.project_version]);
                const group = groups.get(key) ?? []; group.push(entry); groups.set(key, group);
            }
            for (const group of groups.values()) {
                const record = group[0].record;
                const route = record.scope === 'project' ? this.route({ scope: record.scope, projectRoot: record.project_root, projectVersion: record.project_version }) : undefined;
                try {
                    const result = p.object(await this.call({ op: 'bindings_review', items: group.map(e => ({ binding: e.binding, fingerprint: e.review_state.fingerprint, document_revision: e.record.revision })) }, route));
                    confirmed += p.number(result.confirmed);
                } catch (error) { errors.push((route ? '项目共享：' : '个人资料：') + p.messageError(error)); }
            }
            this.hints.invalidate();
            return { confirmed, errors, file: context.label };
        } finally { this.reviewing = false; }
    }
    private async confirmReview(binding: p.Binding, revision: number, fingerprint: string, route?: p.ProjectRef): Promise<unknown> {
        if (await vscode.window.showInformationMessage(`确认 ${binding.path} 的已保存代码与这篇说明仍然一致？`, { modal: true }, '确认仍适用') !== '确认仍适用') return { cancelled: true };
        return this.call({ op: 'binding_review', binding, fingerprint, document_revision: revision }, route);
    }
    private async showChanges(binding: p.Binding, route?: p.ProjectRef): Promise<void> {
        const data = p.object(await this.call({ op: 'binding_changes', binding }, route));
        const text = p.string(data.note) + '\n\n' + (data.diff === null ? '' : p.string(data.diff));
        await vscode.window.showTextDocument(await vscode.workspace.openTextDocument({ content: text, language: data.diff === null ? 'plaintext' : 'diff' }), { preview: true, preserveFocus: true, viewColumn: vscode.ViewColumn.Beside });
    }
    async repairPaths(): Promise<void> {
        const context = await this.resolve(this.capture()); if (!context.root || !context.workspaceId) throw new Error('请先打开本地项目。');
        const scope = await vscode.window.showQuickPick(['个人资料', '项目共享'], { title: '修复文件或目录移动后的关联与片段来源' }); if (!scope) return;
        const project = scope === '项目共享' ? await this.projectContext(context.root) : null;
        if (project?.error) throw new Error(project.error);
        const route = project ? { root: project.root, version: project.version } : undefined;
        const from = await vscode.window.showInputBox({ title: '移动前的项目相对路径', prompt: '输入文件或目录路径，例如 src/payment' }); if (!from) return;
        const to = await vscode.window.showInputBox({ title: '移动后的项目相对路径', prompt: '目标文件或目录必须已经存在。' }); if (!to) return;
        const request = { op: 'paths_repair', workspace_id: context.workspaceId, from, to };
        const preview = p.object(await this.call(request, route));
        if (await vscode.window.showWarningMessage(`将修复 ${p.number(preview.bindings)} 项关联、${p.number(preview.records)} 个片段来源。`, { modal: true }, '修复') !== '修复') return;
        await this.call({ ...request, token: p.string(preview.token) }, route); this.hints.invalidate(); this.post({ type: 'contextChanged' });
    }
    private async ensureIdentity(): Promise<void> {
        if (!p.author(p.object(await this.client.call({ op: 'identity_get' })).author)) {
            if (!(await this.configureIdentity())) throw new p.CoreError('CANCELLED', '未设置共享署名，草稿已保留。');
        }
    }
    async configureIdentity(): Promise<boolean> {
        const existing = p.author(p.object(await this.client.call({ op: 'identity_get' })).author);
        const name = await vscode.window.showInputBox({ title: '设置共享资料署名', value: existing?.display_name ?? '',
            prompt: '此显示名会随共享资料提交到 Git；两端 IDE 共用。署名不代表账号认证。',
            validateInput: value => !value.trim() || [...value.trim()].length > 80 || /[\u0000-\u001f\u007f-\u009f]/u.test(value) ? '请输入 1–80 个字符，不含换行或控制字符。' : undefined });
        if (name === undefined) return false;
        p.author(p.object(await this.client.call({ op: 'identity_set', display_name: name })).author);
        return true;
    }
    async copyCodeLink(): Promise<void> {
        const context = this.capture();
        if (!context.root || !context.path) throw new p.CoreError('NO_FILE', '请先打开项目中的本地源码文件。');
        const format = await vscode.window.showQuickPick([
            { label: 'VS Code 链接', format: 'vscode' }, { label: 'IntelliJ IDEA 链接', format: 'idea' },
            { label: 'Markdown（VS Code + IDEA 两个入口）', format: 'markdown' }
        ], { title: '复制代码位置链接' });
        if (!format) return;
        const links = p.object(await this.client.call({ op: 'code_link_create', root: context.root, path: context.path,
            line: context.line ?? 1, vscode_scheme: vscode.env.uriScheme === 'vscode-insiders' ? 'vscode-insiders' : 'vscode', jetbrains_product: 'idea' }));
        const code = p.string(links.vscode_url); const idea = p.string(links.jetbrains_url);
        const text = format.format === 'markdown' ? `[在 VS Code 中打开代码](${code}) · [在 IDEA 中打开代码](${idea})` : format.format === 'idea' ? idea : code;
        await vscode.env.clipboard.writeText(text);
        void vscode.window.showInformationMessage('已复制代码位置链接。请将 .codemori/project.json 与 .codemori/.gitignore 提交到 Git，同事才能定位自己的克隆。');
    }
    async pasteCodeLink(): Promise<void> {
        const url = await vscode.window.showInputBox({ title: '打开代码位置链接', prompt: '粘贴完整的 CodeMori VS Code 或 JetBrains 链接。' });
        if (url !== undefined) await this.navigateCodeLink(url.trim());
    }
    async handleUri(uri: vscode.Uri): Promise<void> {
        try { await this.navigateCodeLink(uri.toString(true)); if (this.context.extensionMode === vscode.ExtensionMode.Test) this.receivedCodeLinks++; }
        catch (error) { void vscode.window.showErrorMessage(p.messageError(error)); }
    }
    private async navigateCodeLink(url: string): Promise<void> {
        await this.client.call({ op: 'code_link_parse', url });
        const matches: { root: string; path: string; line: number }[] = [];
        const failures: string[] = [];
        for (const folder of vscode.workspace.workspaceFolders ?? []) {
            if (folder.uri.scheme !== 'file') continue;
            try {
                const data = p.object(await this.client.call({ op: 'code_link_resolve', root: folder.uri.fsPath, url })).target;
                if (data !== null) { const target = p.object(data); matches.push({ root: p.string(target.root), path: p.string(target.path), line: p.number(target.line) }); }
            } catch (error) { failures.push(p.messageError(error)); }
        }
        let target = matches.length === 1 ? matches[0] : undefined;
        if (matches.length > 1) {
            target = (await vscode.window.showQuickPick(matches.map(target => ({ label: path.basename(target.root), description: target.root, target })), { title: '选择要打开的项目克隆' }))?.target;
            if (!target) return;
        } else if (!target) {
            if (failures.length) throw new p.CoreError('OPEN_FAILED', failures.join('\n'));
            const folders = await vscode.window.showOpenDialog({ title: '选择包含 .codemori/project.json 的项目克隆', canSelectFiles: false, canSelectFolders: true, canSelectMany: false });
            if (!folders?.length) return;
            if (folders[0].scheme !== 'file') throw new p.CoreError('LOCAL_ONLY', '代码链接仅支持本地工程。');
            const data = p.object(await this.client.call({ op: 'code_link_resolve', root: folders[0].fsPath, url })).target;
            if (data === null) throw new p.CoreError('NOT_FOUND', '所选目录不是链接对应项目；请先拉取已提交的 .codemori/project.json。');
            const value = p.object(data); target = { root: p.string(value.root), path: p.string(value.path), line: p.number(value.line) };
        }
        await this.openFile(vscode.Uri.file(target.path), target.line);
    }
    private async exportBackup(): Promise<unknown> {
        const target = await vscode.window.showSaveDialog({ title: '导出个人资料备份', filters: { JSON: ['json'] }, defaultUri: vscode.Uri.file(path.join(os.homedir(), 'codemori-backup.json')) });
        if (!target) return { cancelled: true };
        if (target.scheme !== 'file') throw new p.CoreError('LOCAL_ONLY', '当前只支持本地备份文件。');
        const backup = await this.client.call({ op: 'backup_export' });
        const temporary = path.join(path.dirname(target.fsPath), `.codemori-backup-${randomUUID()}.json`);
        try { await fs.writeFile(temporary, JSON.stringify(backup, null, 2), { encoding: 'utf8', mode: 0o600 }); await fs.rename(temporary, target.fsPath); }
        finally { await fs.rm(temporary, { force: true }); }
        return { path: target.fsPath };
    }
    private async importBackup(): Promise<unknown> {
        const files = await vscode.window.showOpenDialog({ title: '选择 CodeMori 个人备份', canSelectMany: false, filters: { JSON: ['json'] } });
        if (!files?.length) return { cancelled: true };
        if (files[0].scheme !== 'file') throw new p.CoreError('LOCAL_ONLY', '当前只支持本地备份文件。');
        if ((await fs.stat(files[0].fsPath)).size > 128 * 1024 * 1024) throw new p.CoreError('TOO_LARGE', '备份超过 128 MiB。');
        const backup: unknown = JSON.parse(await fs.readFile(files[0].fsPath, 'utf8'));
        const preview = p.report(await this.client.call({ op: 'backup_preview', backup }));
        if (preview.invalid_count > 0) {
            void vscode.window.showErrorMessage(this.reportSummary(preview));
            return { cancelled: true, invalid_count: preview.invalid_count };
        }
        if (await vscode.window.showWarningMessage(this.reportSummary(preview) + '\n冲突保留本机版本。确认导入？', { modal: true }, '导入') !== '导入') return { cancelled: true };
        const result = p.report(await this.client.call({ op: 'backup_import', backup }));
        void vscode.window.showInformationMessage(this.reportSummary(result));
        return result;
    }
    private reportSummary(report: p.ImportReport): string {
        if (report.invalid_count > 0) return `发现 ${report.invalid_count} 项无效数据，未导入。修复后才能计算新增与冲突数量。`
            + report.invalid_entries.slice(0, 5).map(item => `\n${item.kind}[${item.index}] ${item.id}: ${item.reason}`).join('');
        return `新增资料 ${report.new_records}，工作区 ${report.new_workspaces}，关联 ${report.new_bindings}；跳过相同项 ${report.unchanged}，冲突 ${report.conflicts.length}，无效 0。`
            + report.conflicts.slice(0, 5).map(c => `\n${c.id}: ${c.reason}`).join('');
    }
}

export function activate(context: vscode.ExtensionContext) {
    const application = new CodeMori(context);
    const command = (name: string, action: () => void | Promise<void>) => vscode.commands.registerCommand(name, async () => {
        try { await action(); } catch (error) { void vscode.window.showErrorMessage(p.messageError(error)); }
    });
    context.subscriptions.push(application,
        vscode.window.registerWebviewViewProvider('codemori.library', application, { webviewOptions: { retainContextWhenHidden: true } }),
        command('codemori.open', () => application.open()),
        command('codemori.saveSelection', () => application.saveSelection()),
        command('codemori.associateDocument', () => application.associateDocument()),
        command('codemori.copyCodeLink', () => application.copyCodeLink()),
        command('codemori.openCodeLink', () => application.pasteCodeLink()),
        command('codemori.toggleHints', () => application.toggleHints()),
        command('codemori.repairPaths', () => application.repairPaths()),
        command('codemori.identity', async () => { await application.configureIdentity(); }),
        vscode.window.registerUriHandler(application));
    // Only the isolated Extension Host test runner receives this seam; production exports no API.
    return context.extensionMode === vscode.ExtensionMode.Test ? application.testInterface() : undefined;
}
