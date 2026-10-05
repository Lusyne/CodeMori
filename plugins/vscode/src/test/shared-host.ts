import * as vscode from 'vscode';
import * as assert from 'node:assert/strict';
import * as fs from 'node:fs/promises';
import * as path from 'node:path';
import * as os from 'node:os';
import type { activate } from '../extension';
import { CoreClient, bundledExecutable } from '../core';
import * as p from '../protocol';

/** Opt-in second-IDE verification of a synthetic record saved through the IDEA window. */
export async function run(): Promise<void> {
    const store = process.env.CODEMORI_TEST_DATA_DIR;
    const project = process.env.CODEMORI_TEST_PROJECT;
    const id = process.env.CODEMORI_SHARED_RECORD_ID;
    assert.ok(store && project && id);
    assert.notEqual(path.resolve(store), path.join(os.homedir(), '.codemori'));
    const extension = vscode.extensions.getExtension('lusyne.codemori'); assert.ok(extension);
    const api = await extension.activate() as Exclude<ReturnType<typeof activate>, undefined>;
    const client = new CoreClient(bundledExecutable(extension.extensionPath), store);
    const before = p.record(await client.call({ op: 'record_get', id }));
    const verify = process.env.CODEMORI_SHARED_PHASE === 'verify';
    assert.equal(before.input.title, verify ? 'IDEA 交叉确认' : 'IDEA 界面保存');
    assert.equal(before.input.content, 'return failure == Failure.TIMEOUT && attemptsSoFar < 3;');
    assert.equal(before.input.source?.path, 'PaymentService.java');
    assert.equal(before.input.source?.line, 9);
    assert.ok(before.input.tags.includes('桌面验收'));
    assert.equal(before.input.starred, true);
    const document = await vscode.workspace.openTextDocument(path.join(project, 'PaymentService.java'));
    await vscode.window.showTextDocument(document, vscode.ViewColumn.One);
    await vscode.commands.executeCommand('codemori.open');
    await api.waitForRender();
    const seen = await api.measureSearch(before.input.title);
    assert.equal(seen.total, 1); assert.equal(seen.rows, 1); assert.equal(seen.visible, true);
    const linked = p.array(await client.call({ op: 'file_documents',
        workspace_id: before.input.source!.workspace_id, path: before.input.source!.path }), p.record);
    assert.equal(linked.length, 1);
    assert.equal(linked[0].input.title, '支付设计说明');
    const opened = await api.request({ id: 'shared:source', action: 'open', recordId: id, revision: before.revision });
    assert.equal(opened?.ok, true, opened?.error);
    assert.equal(vscode.window.activeTextEditor?.document.uri.fsPath, path.join(project, 'PaymentService.java'));
    assert.equal(vscode.window.activeTextEditor?.selection.active.line, 8);
    let after = before;
    if (!verify) {
        const edit = await api.request({ id: 'shared:edit', action: 'edit', recordId: id, revision: before.revision });
        assert.equal(edit?.ok, true, edit?.error);
        const draft = api.draft(); assert.ok(draft);
        const saved = await api.request({ id: 'shared:save', action: 'save', token: draft.token,
            input: { ...draft.input, title: 'VS Code 交叉编辑', tags: [...draft.input.tags, '双IDE'] } });
        assert.equal(saved?.ok, true, saved?.error); after = p.record(saved.data);
        assert.equal(after.revision, before.revision + 1);
        const changed = await api.measureSearch('VS Code 交叉编辑');
        assert.equal(changed.total, 1); assert.equal(changed.visible, true);
        const stale = await api.request({ id: 'shared:stale', action: 'favorite', recordId: id, revision: before.revision });
        assert.equal(stale?.ok, false);
    } else assert.ok(before.input.tags.includes('双IDE'));
    const evidence = { vscode: vscode.version, phase: verify ? 'read-idea-edit' : 'read-and-edit-idea-record',
        id, beforeRevision: before.revision, afterRevision: after.revision, title: after.input.title,
        visibleRows: seen.rows, sourcePreserved: true, tags: after.input.tags,
        linkedDocuments: linked.length, sourceOpenedAt: path.join(project, 'PaymentService.java'), sourceLine: 9,
        scope: 'Actual VS Code host and visible webview against the supplied synthetic store. Controller actions use the Test-mode seam; the caller records whether this is the shared IDEA store or a restored copy.' };
    await fs.writeFile(path.join(process.env.CODEMORI_TEST_OUTPUT!, 'shared-ide.json'), JSON.stringify(evidence, null, 2));
    console.log(JSON.stringify(evidence));
}
