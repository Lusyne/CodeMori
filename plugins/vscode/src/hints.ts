import * as vscode from 'vscode';
import * as path from 'node:path';
import { CoreClient } from './core';
import * as p from './protocol';

/** File-header hints use the same core association view as the panel. */
export class DocumentHints implements vscode.CodeLensProvider, vscode.Disposable {
    private readonly changed = new vscode.EventEmitter<void>();
    readonly onDidChangeCodeLenses = this.changed.event;
    private generation = 0;
    private timer?: ReturnType<typeof setTimeout>;
    invalidate(): void { this.generation++; if (this.timer) clearTimeout(this.timer); this.timer = setTimeout(() => this.changed.fire(), 150); }
    constructor(private readonly client: CoreClient) {}
    dispose(): void { if (this.timer) clearTimeout(this.timer); this.changed.dispose(); }
    async provideCodeLenses(document: vscode.TextDocument, token: vscode.CancellationToken): Promise<vscode.CodeLens[]> {
        if (!vscode.workspace.getConfiguration('codemori', document.uri).get('documentHints.enabled', true) || document.uri.scheme !== 'file') return [];
        const folder = vscode.workspace.getWorkspaceFolder(document.uri); if (!folder || folder.uri.scheme !== 'file') return [];
        const relative = path.relative(folder.uri.fsPath, document.uri.fsPath).split(path.sep).join('/');
        const generation = this.generation, version = document.version;
        const controller = new AbortController(); const cancellation = token.onCancellationRequested(() => controller.abort());
        try {
            if (token.isCancellationRequested) return [];
            const workspace = p.workspace(await this.client.call({ op: 'workspace_register', root: folder.uri.fsPath }, controller.signal));
            const data = p.object(await this.client.call({ op: 'library_file_documents', root: workspace.root, workspace_id: workspace.id, path: relative }, controller.signal));
            if (token.isCancellationRequested || generation !== this.generation || version !== document.version || !vscode.workspace.getConfiguration('codemori', document.uri).get('documentHints.enabled', true)) return [];
            const entries = p.array(data.entries, p.association);
            const count = new Set(entries.map(e => `${e.record.scope}:${e.record.id}`)).size;
            if (!count) return [];
            const pending = entries.filter(e => e.review_state.status === 'needs_review').length;
            const inherited = entries.filter(e => e.inherited).length;
            return [new vscode.CodeLens(new vscode.Range(0, 0, 0, 0), { title: `CodeMori · ${count} 篇文档${inherited ? ` · ${inherited} 项模块继承` : ''}${pending ? ` · ${pending} 项待复核` : ''}`,
                command: 'codemori.open', tooltip: '查看当前文件与父目录的关联说明（复核基于已保存代码）' })];
        } catch { return []; } finally { cancellation.dispose(); }
    }
}
