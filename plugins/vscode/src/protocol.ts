export interface Source { workspace_id: string; path: string; line: number | null }
export interface RecordInput {
    kind: 'snippet' | 'document'; title: string; content: string; language: string;
    description: string; tags: string[]; starred: boolean; source: Source | null; url: string | null;
}
export type RecordScope = 'personal' | 'project';
export type LibraryScope = 'all' | RecordScope;
export interface ProjectRef { root: string; version: string }
export interface ProjectInfo extends ProjectRef { file: string; exists: boolean; error: string | null }
export function libraryScope(value: unknown): LibraryScope {
    if (value === undefined || value === 'all') return 'all';
    if (value === 'personal' || value === 'project') return value;
    throw new CoreError('INVALID_REQUEST', 'Invalid library scope.');
}
export function recordScope(value: unknown): RecordScope {
    if (value === undefined || value === 'personal') return 'personal';
    if (value === 'project') return 'project';
    throw new CoreError('INVALID_REQUEST', 'Invalid record scope.');
}
export function projectInfo(value: unknown): ProjectInfo | null {
    if (value == null) return null;
    const v = object(value);
    return { root: string(v.root), version: string(v.version), file: string(v.file), exists: boolean(v.exists), error: v.error == null ? null : string(v.error) };
}
export interface Author { id: string; display_name: string }
export function author(value: unknown): Author | null {
    if (value == null) return null;
    const v = object(value); return { id: string(v.id), display_name: string(v.display_name) };
}
export interface KnowledgeRecord {
    id: string; revision: number; created_at: number; updated_at: number; is_demo: boolean; input: RecordInput;
    scope: RecordScope; project_root?: string; project_version?: string; created_by?: Author | null; updated_by?: Author | null;
}
export interface Workspace { id: string; name: string; root: string; revision: number }
export interface Review { fingerprint: string; confirmed_at: number; confirmed_by: Author | null; git_commit: string | null; path: string }
export interface Binding { workspace_id: string; path: string; document_id: string; kind?: 'file' | 'module'; review?: Review | null }
export interface Association { record: KnowledgeRecord; binding: Binding; inherited: boolean; review_state: { status: string; fingerprint: string | null; error: string | null } }
export function bindingKind(value: unknown): 'file' | 'module' { if (value === undefined || value === 'file') return 'file'; if (value === 'module') return 'module'; throw new CoreError('INVALID_REQUEST', 'Invalid association target.'); }
export function association(value: unknown): Association {
    const v = object(value), state = object(v.review_state);
    const status = string(state.status); if (!['current', 'needs_review', 'unconfirmed', 'unavailable'].includes(status)) throw new CoreError('INVALID_RESPONSE', 'Invalid review state.');
    return { record: record(v.record), binding: binding(v.binding), inherited: boolean(v.inherited), review_state: { status, fingerprint: state.fingerprint == null ? null : string(state.fingerprint), error: state.error == null ? null : string(state.error) } };
}
export interface DocumentOpenTargets { original_url: string; feishu_applink: string | null }
export function documentOpenTargets(value: unknown): DocumentOpenTargets {
    const v = object(value);
    return { original_url: string(v.original_url), feishu_applink: v.feishu_applink === null ? null : string(v.feishu_applink) };
}
export interface MarkdownPreview { title: string; path: string; html: string }
export function markdownPreview(value: unknown): MarkdownPreview {
    const v = object(value); return { title: string(v.title), path: string(v.path), html: string(v.html) };
}
export interface SearchFilter {
    query: string; kind?: 'snippet' | 'document' | 'comment'; tag?: string; workspace_id?: string;
    starred: boolean; demo: boolean; limit: number; offset: number;
}
export interface SearchHit { record: KnowledgeRecord; workspace_names: string[]; excerpt: { text: string; highlight: boolean }[] }
export interface SearchPage { items: SearchHit[]; total: number; limit: number; offset: number }
export interface CommentHit { source: Source; text: string; language: string; workspace_name: string; indexed_at: number; end_line: number; excerpt: { text: string; highlight: boolean }[] }
export interface CommentPage { items: CommentHit[]; total: number; limit: number; offset: number }
export function commentPage(value: unknown): CommentPage {
    const v = object(value);
    return { total: number(v.total), limit: number(v.limit), offset: number(v.offset), items: array(v.items, value => {
        const c = object(value); return { source: source(c.source), text: string(c.text), language: string(c.language),
            workspace_name: string(c.workspace_name), indexed_at: number(c.indexed_at), end_line: number(c.end_line),
            excerpt: array(c.excerpt, value => { const span = object(value); return { text: string(span.text), highlight: boolean(span.highlight) }; }) };
    }) };
}
export function indexStatus(value: unknown) {
    const v = object(value); return { files: number(v.files), comments: number(v.comments), indexed_at: v.indexed_at == null ? null : number(v.indexed_at) };
}
export function indexReport(value: unknown) {
    const v = object(value); return { indexed_files: number(v.indexed_files), unchanged_files: number(v.unchanged_files),
        removed_files: number(v.removed_files), skipped_files: number(v.skipped_files), failed_files: number(v.failed_files),
        details: array(v.details, string), status: indexStatus(v.status) };
}
export interface ImportReport {
    new_workspaces: number; new_records: number; new_bindings: number; unchanged: number;
    conflicts: { kind: string; id: string; reason: string }[];
    invalid_count: number;
    invalid_entries: { kind: string; index: number; id: string; reason: string }[];
}
export class CoreError extends Error {
    constructor(readonly code: string, message: string) { super(message); this.name = 'CoreError'; }
}
export function object(value: unknown): Record<string, unknown> {
    if (!value || typeof value !== 'object' || Array.isArray(value)) throw new CoreError('INVALID_RESPONSE', 'Expected an object.');
    return value as Record<string, unknown>;
}
export function string(value: unknown): string {
    if (typeof value !== 'string') throw new CoreError('INVALID_RESPONSE', 'Expected a string.');
    return value;
}
export function number(value: unknown): number {
    if (typeof value !== 'number' || !Number.isSafeInteger(value)) throw new CoreError('INVALID_RESPONSE', 'Expected a safe integer.');
    return value;
}
export function boolean(value: unknown): boolean {
    if (typeof value !== 'boolean') throw new CoreError('INVALID_RESPONSE', 'Expected a boolean.');
    return value;
}
export function array<T>(value: unknown, parse: (value: unknown) => T): T[] {
    if (!Array.isArray(value)) throw new CoreError('INVALID_RESPONSE', 'Expected an array.');
    return value.map(parse);
}
export function decode(text: string): unknown {
    let root: Record<string, unknown>;
    try { root = object(JSON.parse(text)); }
    catch { throw new CoreError('INVALID_RESPONSE', 'CodeMori returned invalid JSON.'); }
    if (root.protocol_version !== 1) throw new CoreError('PROTOCOL_UNSUPPORTED', 'Update CodeMori: unsupported core protocol.');
    if (!boolean(root.ok)) { const error = object(root.error); throw new CoreError(string(error.code), string(error.message)); }
    if (root.data === undefined || root.data === null) throw new CoreError('INVALID_RESPONSE', 'Missing response data.');
    return root.data;
}
export function source(value: unknown): Source {
    const v = object(value); return { workspace_id: string(v.workspace_id), path: string(v.path), line: v.line == null ? null : number(v.line) };
}
export function input(value: unknown): RecordInput {
    const v = object(value);
    if (v.kind !== 'snippet' && v.kind !== 'document') throw new CoreError('INVALID_RECORD', 'Unsupported record kind.');
    return { kind: v.kind, title: string(v.title ?? ''), content: string(v.content ?? ''), language: string(v.language ?? ''),
        description: string(v.description ?? ''), tags: array(v.tags ?? [], string), starred: boolean(v.starred ?? false),
        source: v.source == null ? null : source(v.source), url: v.url == null ? null : string(v.url) };
}
export function record(value: unknown): KnowledgeRecord {
    const v = object(value); const revision = number(v.revision);
    const fields = object(v.input);
    for (const name of ['title', 'content', 'language', 'description']) string(fields[name]);
    array(fields.tags, string); boolean(fields.starred);
    if (!Object.hasOwn(fields, 'source') || !Object.hasOwn(fields, 'url')) throw new CoreError('INVALID_RECORD', 'Missing record fields.');
    if (revision < 1) throw new CoreError('INVALID_RECORD', 'Missing record revision.');
    const scope = recordScope(v.scope);
    return { id: string(v.id), revision, created_at: number(v.created_at), updated_at: number(v.updated_at), is_demo: boolean(v.is_demo), input: input(v.input), scope,
        created_by: author(v.created_by), updated_by: author(v.updated_by),
        project_root: scope === 'project' ? string(v.project_root) : undefined,
        project_version: scope === 'project' ? string(v.project_version) : undefined };
}
export function workspace(value: unknown): Workspace {
    const v = object(value); return { id: string(v.id), name: string(v.name), root: string(v.root), revision: number(v.revision) };
}
export function binding(value: unknown): Binding {
    const v = object(value); const result: Binding = { workspace_id: string(v.workspace_id), path: string(v.path), document_id: string(v.document_id), kind: bindingKind(v.kind) };
    if (v.review != null) { const r = object(v.review); result.review = { fingerprint: string(r.fingerprint), confirmed_at: number(r.confirmed_at), confirmed_by: author(r.confirmed_by), git_commit: r.git_commit == null ? null : string(r.git_commit), path: string(r.path) }; }
    return result;
}
export function page(value: unknown): SearchPage {
    const v = object(value);
    return { total: number(v.total), limit: number(v.limit), offset: number(v.offset), items: array(v.items, value => {
        const hit = object(value); return { record: record(hit.record), workspace_names: array(hit.workspace_names ?? [], string),
            excerpt: array(hit.excerpt, value => { const span = object(value); return { text: string(span.text), highlight: boolean(span.highlight) }; }) };
    }) };
}
export function report(value: unknown): ImportReport {
    const v = object(value); return { new_workspaces: number(v.new_workspaces), new_records: number(v.new_records),
        invalid_count: number(v.invalid_count), invalid_entries: array(v.invalid_entries, value => {
            const item = object(value); return { kind: string(item.kind), index: number(item.index), id: string(item.id), reason: string(item.reason) };
        }),
        new_bindings: number(v.new_bindings), unchanged: number(v.unchanged), conflicts: array(v.conflicts, value => {
            const c = object(value); return { kind: string(c.kind), id: string(c.id), reason: string(c.reason) };
        }) };
}
export function uiFilter(value: unknown): SearchFilter {
    const v = object(value); const kind = v.kind;
    if (kind !== undefined && kind !== '' && kind !== 'snippet' && kind !== 'document' && kind !== 'comment') throw new CoreError('INVALID_REQUEST', 'Invalid source filter.');
    return { query: string(v.query ?? ''), kind: kind || undefined, tag: v.tag ? string(v.tag) : undefined,
        starred: boolean(v.starred ?? false), demo: false, limit: 50, offset: number(v.offset ?? 0) };
}
export function messageError(error: unknown): string {
    if (error instanceof CoreError && error.code === 'CONFLICT' && error.message.includes('Project shared file changed')) return '项目共享文件已变更，当前草稿仍保留。请复制需要的内容后刷新，再重新编辑。';
    if (error instanceof CoreError && error.code === 'CONFLICT') return '记录已被其他客户端修改。当前草稿仍保留，请刷新并比较最新内容后再保存。';
    return error instanceof Error ? error.message : String(error);
}
