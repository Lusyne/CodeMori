import { snippetHtml } from '../webview';
import { feishuOpenCommand } from '../feishu';
import * as vscode from 'vscode';
import * as assert from 'node:assert/strict';
import * as fs from 'node:fs/promises';
import * as path from 'node:path';
import { CoreClient, bundledExecutable } from '../core';
import * as p from '../protocol';
import type { activate } from '../extension';
import { verifyExternalOpen } from './external-open';

export async function run(): Promise<void> {
    const unsafeSnapshot = snippetHtml('<img src=x>', '</code><script>alert(1)</script>');
    assert.ok(!unsafeSnapshot.includes('<script>')); assert.ok(unsafeSnapshot.includes('&lt;script&gt;'));
    assert.ok(unsafeSnapshot.includes("default-src 'none'"));
    const extension = vscode.extensions.getExtension('lusyne.codemori');
    assert.ok(extension); const api = await extension.activate() as Exclude<ReturnType<typeof activate>, undefined>; assert.equal(extension.isActive, true);
    const commands = await vscode.commands.getCommands(true);
    for (const command of ['codemori.open', 'codemori.saveSelection', 'codemori.associateDocument']) assert.ok(commands.includes(command));
    const project = process.env.CODEMORI_TEST_PROJECT!;
    const document = await vscode.workspace.openTextDocument(vscode.Uri.file(path.join(project, 'PaymentService.java')));
    const editor = await vscode.window.showTextDocument(document, vscode.ViewColumn.One);
    editor.selection = new vscode.Selection(1, 0, 3, 0);
    // Opening the activity-bar container must resolve a real webview view, not an editor tab.
    await vscode.commands.executeCommand('workbench.view.extension.codemori');
    await api.waitForRender();
    assert.deepEqual(api.sidebar(), { visible: true, viewType: 'codemori.library' });
    assert.ok(!vscode.window.tabGroups.all.some(group => group.tabs.some(tab => tab.label === 'CodeMori' && tab.input instanceof vscode.TabInputWebview)));
    assert.equal(vscode.window.activeTextEditor?.document.uri.fsPath, document.uri.fsPath);
    const contribution = extension.packageJSON.contributes;
    assert.equal(contribution.viewsContainers.activitybar[0].id, 'codemori');
    assert.equal(contribution.views.codemori[0].type, 'webview');
    await fs.access(path.join(extension.extensionPath, contribution.viewsContainers.activitybar[0].icon));
    const store = process.env.CODEMORI_TEST_DATA_DIR!;
    // Actual webview startup must reach the host controller and native RPC without an extra client.
    const deadline = Date.now() + 15_000;
    while (Date.now() < deadline) {
        try { await fs.access(path.join(store, 'codemori.sqlite3')); break; }
        catch { await new Promise(resolve => setTimeout(resolve, 100)); }
    }
    await fs.access(path.join(store, 'codemori.sqlite3'));
    const client = new CoreClient(bundledExecutable(extension.extensionPath), store);
    await vscode.commands.executeCommand('workbench.action.closeSidebar');
    await vscode.commands.executeCommand('codemori.saveSelection');
    assert.equal(api.sidebar().visible, true, 'Capture must reveal the sidebar before delivering its draft');
    const draft = api.draft(); assert.ok(draft); assert.match(draft.input.content, /支付重试/); assert.ok(draft.input.source);
    await vscode.commands.executeCommand('workbench.action.closeSidebar');
    const otherPath = path.join(project, 'Other.java'); await fs.writeFile(otherPath, 'class Other {}');
    await vscode.window.showTextDocument(await vscode.workspace.openTextDocument(vscode.Uri.file(otherPath)), vscode.ViewColumn.One);
    await vscode.commands.executeCommand('codemori.open');
    const visibleDeadline = Date.now() + 5000;
    while (!api.sidebar().visible && Date.now() < visibleDeadline) await new Promise(resolve => setTimeout(resolve, 25));
    assert.equal(api.sidebar().visible, true);
    assert.equal(api.draft()?.token, draft.token, 'Hiding the sidebar must retain its pending draft');
    assert.equal(api.draft()?.input.source?.path, 'PaymentService.java', 'Changing editor must not retarget a captured draft');
    const switched = await api.request({ id: 'host:switched', action: 'search', filter: {} });
    assert.equal(p.object(switched?.data).file, 'Other.java');
    await vscode.window.showTextDocument(document, vscode.ViewColumn.One);
    editor.selection = new vscode.Selection(1, 0, 3, 0);
    const created = await api.request({ id: 'host:save', action: 'save', token: draft.token, input: { ...draft.input, title: 'UI pipeline snapshot', tags: ['重试'] } });
    assert.equal(created?.ok, true); const fromUi = p.record(created.data);
    const fullSnapshot = await api.request({ id: 'host:snapshot', action: 'previewSnippet', recordId: fromUi.id, revision: fromUi.revision });
    assert.equal(fullSnapshot?.ok, true);
    const snapshotDeadline = Date.now() + 5000;
    while (!vscode.window.tabGroups.all.some(group => group.tabs.some(tab => tab.label === '代码快照 · UI pipeline snapshot')) && Date.now() < snapshotDeadline) await new Promise(resolve => setTimeout(resolve, 25));
    assert.ok(vscode.window.tabGroups.all.some(group => group.tabs.some(tab => tab.label === '代码快照 · UI pipeline snapshot')));
    assert.equal((await client.call({ op: 'record_get', id: fromUi.id }) as {revision: number}).revision, fromUi.revision);
    const staleSnapshot = await api.request({ id: 'host:stale-snapshot', action: 'previewSnippet', recordId: fromUi.id, revision: fromUi.revision + 1 });
    assert.equal(staleSnapshot?.ok, false);
    await vscode.window.showTextDocument(document, vscode.ViewColumn.One);
    const search = await api.request({ id: 'host:search', action: 'search', filter: { query: 'UI pipeline', offset: 0 }, reloadTags: true });
    assert.equal(search?.ok, true); assert.equal(p.page(p.object(search.data).result).total, 1);
    const favorite = await api.request({ id: 'host:favorite', action: 'favorite', recordId: fromUi.id, revision: fromUi.revision });
    assert.equal(favorite?.ok, true); const favoriteRecord = p.record(favorite.data); assert.equal(favoriteRecord.input.starred, true);
    const stale = await api.request({ id: 'host:stale', action: 'favorite', recordId: fromUi.id, revision: fromUi.revision });
    assert.equal(stale?.ok, false);
    const priorClipboard = await vscode.env.clipboard.readText();
    try {
        const copy = await api.request({ id: 'host:copy', action: 'copy', recordId: fromUi.id, revision: favoriteRecord.revision });
        assert.equal(copy?.ok, true); assert.ok(await vscode.env.clipboard.readText() === draft.input.content, 'Clipboard did not match the captured snapshot');
    } finally {
        if (await vscode.env.clipboard.readText() === draft.input.content) await vscode.env.clipboard.writeText(priorClipboard);
    }
    await vscode.commands.executeCommand('codemori.associateDocument');
    const docDraft = api.draft(); assert.ok(docDraft?.binding);
    const linked = await api.request({ id: 'host:link', action: 'save', token: docDraft.token,
        input: { ...docDraft.input, title: 'Design note', url: 'https://example.com/host-note', description: '手写摘要：支付重试' } });
    assert.equal(linked?.ok, true);
    const fileDocs = p.array(await client.call({ op: 'file_documents', workspace_id: docDraft.binding.workspace_id, path: docDraft.binding.path }), p.record);
    assert.equal(fileDocs.length, 1); assert.equal(fileDocs[0].input.description, '手写摘要：支付重试');
    for (const action of ['demoInstall', 'demoRemove']) assert.equal((await api.request({ id: 'host:removed-' + action, action }))?.ok, false);
    const forbidden = await api.request({ id: 'host:forbidden', action: 'arbitrary_rpc', op: 'delete_all' });
    assert.equal(forbidden?.ok, false);
    const workspace = p.workspace(await client.call({ op: 'workspace_register', root: project }));
    const saved = p.record(await client.call({ op: 'record_create', record: { kind: 'snippet', title: 'Extension Host 样例', content: 'retry();', source: { workspace_id: workspace.id, path: 'PaymentService.java', line: 3 } } }));
    assert.ok(p.page(await client.call({ op: 'search', filter: { query: 'Extension Host' } })).items.some(hit => hit.record.id === saved.id));
    const indexed = await api.request({ id: 'host:index', action: 'indexWorkspace' });
    assert.equal(indexed?.ok, true);
    assert.ok(p.indexReport(indexed.data).status.comments > 0);
    const indexedSearch = await api.request({ id: 'host:comments', action: 'search', filter: { kind: 'comment', query: '支付重试' } });
    assert.equal(indexedSearch?.ok, true);
    const commentPage = p.commentPage(p.object(indexedSearch.data).comments); assert.ok(commentPage.total > 0);
    const jump = await api.request({ id: 'host:jump', action: 'openComment', source: commentPage.items[0].source });
    assert.equal(jump?.ok, true);
    assert.equal(vscode.window.activeTextEditor?.selection.start.line, commentPage.items[0].source.line! - 1);
    const notePath = path.join(project, 'preview.md');
    const markdown = '# Host readonly note\n\n**bold**';
    await fs.writeFile(notePath, markdown);
    const note = p.record(await client.call({ op: 'record_create', record: { kind: 'document', title: 'Local preview', url: notePath } }));
    const preview = await api.request({ id: 'host:markdown', action: 'markdown', recordId: note.id, revision: note.revision });
    assert.equal(preview?.ok, true);
    const previewDeadline = Date.now() + 5000;
    while (!vscode.window.tabGroups.all.some(group => group.tabs.some(tab => tab.label === '只读预览 · Local preview')) && Date.now() < previewDeadline) {
        await new Promise(resolve => setTimeout(resolve, 50));
    }
    assert.ok(vscode.window.tabGroups.all.some(group => group.tabs.some(tab => tab.label === '只读预览 · Local preview')));
    assert.equal(await fs.readFile(notePath, 'utf8'), markdown);
    const original = await api.request({ id: 'host:local-original', action: 'open', recordId: note.id, revision: note.revision });
    assert.equal(original?.ok, true);
    assert.ok(vscode.window.activeTextEditor);
    assert.equal(await fs.realpath(vscode.window.activeTextEditor.document.uri.fsPath), await fs.realpath(notePath));
    const feishu = p.record(await client.call({ op: 'record_create', record: {
        kind: 'document', title: 'Feishu AppLink fixture', url: 'https://tenant.feishu.cn/wiki/abc?from=ide#anchor' } }));
    const targets = await api.request({ id: 'host:feishu-targets', action: 'documentTargets', recordId: feishu.id, revision: feishu.revision });
    assert.equal(targets?.ok, true, targets?.error);
    const opening = p.documentOpenTargets(targets.data);
    assert.equal(opening.original_url, feishu.input.url);
    assert.equal(opening.feishu_applink, 'feishu://applink.feishu.cn/client/web_url/open?mode=window&url=https%3A%2F%2Ftenant.feishu.cn%2Fwiki%2Fabc%3Ffrom%3Dide%23anchor');
    assert.equal(feishuOpenCommand(opening.feishu_applink!)[1].at(-1), opening.feishu_applink);
    const invalidFeishu = await api.request({ id: 'host:feishu-invalid', action: 'openFeishu', recordId: note.id, revision: note.revision });
    assert.equal(invalidFeishu?.ok, false);
    const staleFeishu = await api.request({ id: 'host:feishu-stale', action: 'openFeishu', recordId: feishu.id, revision: feishu.revision + 1 });
    assert.equal(staleFeishu?.ok, false);
    if (process.env.CODEMORI_TEST_FEISHU_URL) {
        const record = p.record(await client.call({ op: 'record_create', record: {
            kind: 'document', title: 'Feishu client acceptance', url: process.env.CODEMORI_TEST_FEISHU_URL } }));
        const opened = await api.request({ id: 'host:feishu-client', action: 'openFeishu', recordId: record.id, revision: record.revision });
        assert.equal(opened?.ok, true, opened?.error);
        const unchanged = p.record(await client.call({ op: 'record_get', id: record.id }));
        assert.deepEqual(unchanged, record);
        await fs.writeFile(path.join(process.env.CODEMORI_TEST_OUTPUT!, 'feishu-open.json'), JSON.stringify({
            vscode: vscode.version, controllerReportedSuccess: true, savedRecordUnchanged: true,
            scheme: 'feishu', host: 'applink.feishu.cn', path: '/client/web_url/open', mode: 'window',
            scope: 'User-authorized document URL dispatched from the production controller in Test mode. URL omitted; receiving-client confirmation must be recorded separately.'
        }, null, 2));
    }
    if (process.env.CODEMORI_TEST_EXTERNAL_OPEN === '1') await verifyExternalOpen(api, client);
    if (process.env.CODEMORI_TEST_OBSIDIAN_NOTE) {
        const note = path.resolve(process.env.CODEMORI_TEST_OBSIDIAN_NOTE);
        const before = await fs.readFile(note, 'utf8');
        const url = `obsidian://open?path=${encodeURIComponent(note)}&paneType=tab`;
        const record = p.record(await client.call({ op: 'record_create', record: { kind: 'document', title: 'Obsidian navigation acceptance', url } }));
        const opened = await api.request({ id: 'host:obsidian-original', action: 'open', recordId: record.id, revision: record.revision });
        assert.equal(opened?.ok, true, opened?.error);
        assert.equal(await fs.readFile(note, 'utf8'), before);
        await fs.writeFile(path.join(process.env.CODEMORI_TEST_OUTPUT!, 'obsidian-open.json'), JSON.stringify({
            url, note, controllerReportedSuccess: true, contentUnchangedAtDispatch: true,
            scope: 'OS accepted Obsidian navigation from the production document controller in Test mode; target-app confirmation must be recorded separately.'
        }, null, 2));
    }
    const tabs = vscode.window.tabGroups.all.flatMap(group => group.tabs).filter(tab => tab.input instanceof vscode.TabInputWebview);
    await vscode.window.tabGroups.close(tabs);
    await fs.writeFile(path.join(process.env.CODEMORI_TEST_OUTPUT!, 'sidebar-result.json'), JSON.stringify({ activityBarView: true, noEditorPanel: true, sourcePreserved: true, hiddenDraftPreserved: true, fileSwitch: true, snapshotPreview: true, staleSnapshotRefused: true, demoActionsRemoved: true }, null, 2));
    console.log('CodeMori Extension Host: sidebar lifecycle, activation, webview startup, native selection, save/search/favorite/copy/link, comment indexing/search/navigation, readonly Markdown preview, stale-version rejection and message whitelist passed.');
}
