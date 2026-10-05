import * as vscode from 'vscode';
import assert from 'node:assert/strict';
import * as fs from 'node:fs/promises';
import * as path from 'node:path';
import * as os from 'node:os';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { createHash } from 'node:crypto';
import { CoreClient, bundledExecutable } from '../core';
import * as p from '../protocol';
import type { activate } from '../extension';

export async function run(): Promise<void> {
    const qa = process.env.CODEMORI_PROJECT_SHARING_QA!;
    const project = process.env.CODEMORI_TEST_PROJECT!;
    const store = process.env.CODEMORI_TEST_DATA_DIR!;
    const phase = process.env.CODEMORI_PROJECT_SHARING_PHASE!;
    assert.ok(project.startsWith(qa + path.sep) && store.startsWith(qa + path.sep));
    assert.notEqual(path.resolve(store), path.join(os.homedir(), '.codemori'));
    const extension = vscode.extensions.getExtension('lusyne.codemori'); assert.ok(extension);
    const api = await extension.activate() as Exclude<ReturnType<typeof activate>, undefined>;
    const client = new CoreClient(bundledExecutable(extension.extensionPath), store);
    await client.call({ op: "identity_set", display_name: phase === "write" ? "Writer QA" : "Reader QA" });
    if (phase === 'write') {
        const links = p.object(await client.call({ op: 'code_link_create', root: project, path: 'src/中文 +#%.rs', line: 2 }));
        await fs.writeFile(path.join(qa, 'code-link.json'), JSON.stringify(links));
    } else {
        const links = JSON.parse(await fs.readFile(path.join(qa, 'code-link.json'), 'utf8'));
        const uri = vscode.Uri.parse(p.string(links.vscode_url));
        await api.receiveCodeLink(uri.toString(true));
        assert.equal(vscode.window.activeTextEditor?.document.uri.fsPath, path.join(project, 'src/中文 +#%.rs'));
        assert.equal(vscode.window.activeTextEditor?.selection.start.line, 1);
        await api.receiveCodeLink(p.string(links.jetbrains_url));
        assert.equal(vscode.window.activeTextEditor?.selection.start.line, 1);
        let realUriDispatch = false;
        if (process.env.CODEMORI_TEST_URI_DISPATCH === '1') {
            const executable = process.env.CODEMORI_VSCODE_EXECUTABLE;
            assert.ok(executable, 'Explicit installed IDE required for URI dispatch');
            const profile = process.env.CODEMORI_TEST_OUTPUT!;
            assert.ok(profile.startsWith(path.join(extension.extensionPath, '.vscode-test') + path.sep));
            await vscode.commands.executeCommand('workbench.action.closeActiveEditor');
            const launchEnv = { ...process.env }; delete launchEnv.ELECTRON_RUN_AS_NODE;
            await promisify(execFile)(executable, ['--user-data-dir', path.join(profile, 'user'), '--extensions-dir', path.join(profile, 'extensions'), '--open-url', p.string(links.vscode_url)], { timeout: 10000, env: launchEnv });
            const deadline = Date.now() + 45000;
            while (api.receivedCodeLinks() === 0 && Date.now() < deadline) await new Promise(resolve => setTimeout(resolve, 100));
            assert.ok(api.receivedCodeLinks() > 0, 'Registered URI handler did not receive the external command');
            assert.equal(vscode.window.activeTextEditor?.document.uri.fsPath, path.join(project, 'src/中文 +#%.rs'));
            assert.equal(vscode.window.activeTextEditor?.selection.start.line, 1);
            realUriDispatch = true;
        }
        await fs.writeFile(path.join(qa, 'code-link-result.json'), JSON.stringify({ encodedPathPreserved: true, cloneResolved: true, bothIdeFlavors: true, line: 2, realUriDispatch }));
    }
    const file = path.join(project, 'PaymentService.java');
    const document = await vscode.workspace.openTextDocument(file);
    const editor = await vscode.window.showTextDocument(document, vscode.ViewColumn.One);
    editor.selection = new vscode.Selection(1, 0, 3, 0);
    await vscode.commands.executeCommand('codemori.open'); await api.waitForRender();
    let counter = 0;
    const request = async (action: string, payload: object = {}) => {
        const response = await api.request({ id: 'project-test:' + (++counter), action, ...payload });
        assert.equal(response?.ok, true, response?.error); return response.data;
    };
    const route = (record: p.KnowledgeRecord) => ({ recordId: record.id, revision: record.revision, scope: record.scope,
        projectRoot: record.project_root, projectVersion: record.project_version });
    const search = async (libraryScope = 'all') => p.object(await request('search', { filter: { query: '' }, libraryScope, reloadTags: true }));
    const sharedPath = path.join(project, '.codemori/shared.json');
    if (phase === 'write') {
        await vscode.commands.executeCommand('codemori.saveSelection');
        let draft = api.draft(); assert.ok(draft?.project); assert.equal(draft.project.exists, false);
        const personal = p.record(await request('save', { token: draft.token, scope: 'personal', input: { ...draft.input, title: 'PRIVATE_ONLY_SENTINEL', tags: ['only-personal'] } }));
        await request('favorite', route(personal));
        await assert.rejects(fs.access(sharedPath));
        await vscode.commands.executeCommand('codemori.saveSelection'); draft = api.draft(); assert.ok(draft?.project);
        const shared = p.record(await request('save', { token: draft.token, scope: 'project', input: { ...draft.input, title: '团队重试', tags: ['团队'] } }));
        assert.equal(shared.created_by?.display_name, 'Writer QA'); assert.equal(shared.updated_by?.display_name, 'Writer QA');
        assert.equal(shared.scope, 'project'); assert.equal(shared.input.source?.path, 'PaymentService.java');
        for (const [title, url] of [['团队设计', path.join(project, 'docs/guide.md')], ['团队飞书', 'https://tenant.feishu.cn/wiki/demo']]) {
            await vscode.commands.executeCommand('codemori.associateDocument'); draft = api.draft(); assert.ok(draft?.project && draft.binding);
            await request('save', { token: draft.token, scope: 'project', input: { ...draft.input, title, url, description: '团队手填摘要', tags: ['团队'] } });
        }
        const result = await search(); assert.equal(p.page(result.result).total, 4);
        assert.equal(p.array(result.documents, p.record).filter(r => r.scope === 'project').length, 2);
        assert.equal(p.page((await search('personal')).result).total, 1);
        assert.equal(p.page((await search('project')).result).total, 3);
        const records = p.page(result.result).items.map(hit => hit.record);
        const current = records.find(r => r.id === shared.id && r.scope === 'project')!;
        const favorite = await api.request({ id: 'project-test:favorite-refused', action: 'favorite', ...route(current) }); assert.equal(favorite?.ok, false);
        const forged = await api.request({ id: 'project-test:root-refused', action: 'documentTargets', ...route(current), projectRoot: path.join(qa, 'not-opened') }); assert.equal(forged?.ok, false);
        await request('edit', route(current)); draft = api.draft(); assert.ok(draft);
        const manifest = JSON.parse(await fs.readFile(sharedPath, 'utf8'));
        manifest.records.find((r: {id: string}) => r.id === shared.id).input.description = 'Changed by Git with the same record revision';
        const changed = JSON.stringify(manifest, null, 2); await fs.writeFile(sharedPath, changed);
        const stale = await api.request({ id: 'project-test:stale', action: 'save', token: draft.token, scope: 'project', input: { ...draft.input, title: 'Local unsaved draft' } });
        assert.equal(stale?.ok, false); assert.equal(api.draft()?.token, draft.token); assert.equal(await fs.readFile(sharedPath, 'utf8'), changed);
        await request('discard');
        const visible = await api.measureSearch('团队'); assert.equal(visible.total, 3); assert.equal(visible.visible, true);
        const raw = await fs.readFile(sharedPath, 'utf8');
        assert.ok(!raw.includes('PRIVATE_ONLY_SENTINEL') && !raw.includes('only-personal') && !raw.includes(project));
        assert.ok(!raw.includes('workspace_id') && !raw.includes('starred'));
        await fs.writeFile(path.join(qa, 'write.json'), JSON.stringify({ vscode: vscode.version, privateRecords: 1, sharedRecords: 3, sharedBindings: 2,
            explicitScope: true, defaultPersonalCreatedNoSharedFile: true, staleDraftPreserved: true, foreignRootRefused: true, visibleRows: visible.rows }, null, 2));
    } else {
        assert.equal(p.page(await client.call({ op: 'search' })).total, 0);
        const data = await search(); const page = p.page(data.result); assert.equal(page.total, 3);
        assert.ok(page.items.every(hit => hit.record.scope === 'project'));
        const documents = p.array(data.documents, p.record); assert.equal(documents.length, 2);
        const snippet = page.items.find(hit => hit.record.input.kind === 'snippet')!.record;
        assert.equal(snippet.created_by?.display_name, 'Writer QA');
        const note = documents.find(record => record.input.url === './docs/guide.md')!;
        const body = '# CLONE B LOCAL BODY\n'; await fs.writeFile(path.join(project, 'docs/guide.md'), body);
        await request('open', route(note));
        assert.equal(vscode.window.activeTextEditor?.document.uri.fsPath, path.join(project, 'docs/guide.md'));
        assert.equal(vscode.window.activeTextEditor?.document.getText(), body);
        await request('open', route(snippet));
        assert.equal(vscode.window.activeTextEditor?.document.uri.fsPath, file);
        assert.equal(vscode.window.activeTextEditor?.selection.active.line, 1);
        const clipboard = await vscode.env.clipboard.readText();
        try { await request('copy', route(snippet)); assert.ok(await vscode.env.clipboard.readText() === snippet.input.content, 'Shared snapshot copy did not match'); }
        finally { if (await vscode.env.clipboard.readText() === snippet.input.content) await vscode.env.clipboard.writeText(clipboard); }
        const visible = await api.measureSearch(''); assert.equal(visible.total, 3); assert.equal(visible.visible, true);
        const oldVersion = api.projectVersion(); assert.ok(oldVersion);
        const origin = path.join(qa, 'origin');
        const otherClient = new CoreClient(bundledExecutable(extension.extensionPath), path.join(qa, 'personal-a'));
        const originInfo = p.projectInfo(await otherClient.call({ op: 'project_info', root: origin })); assert.ok(originInfo);
        const originRecord = p.record(await otherClient.call({ op: 'project', root: origin, request: { op: 'record_get', id: snippet.id } }));
        const run = promisify(execFile);
        const git = async (directory: string, args: string[]) => run('git', ['-C', directory, '-c', 'core.hooksPath=' + path.join(qa, 'empty-hooks'),
            '-c', 'commit.gpgsign=false', '-c', 'user.name=CodeMori QA', '-c', 'user.email=qa@example.invalid', ...args]);
        const beforeCommit = (await git(origin, ['rev-parse', 'HEAD'])).stdout;
        await otherClient.call({ op: 'project_save', root: origin, expected_version: originInfo.version,
            id: originRecord.id, revision: originRecord.revision, record: { ...originRecord.input, title: '团队更新 after pull' } });
        assert.equal((await git(origin, ['rev-parse', 'HEAD'])).stdout, beforeCommit, 'Plugin must not commit automatically');
        await git(origin, ['add', '.codemori/shared.json']); await git(origin, ['commit', '-m', 'Update teammate shared fixture']);
        await git(project, ['-c', 'protocol.file.allow=always', 'pull', '--ff-only']);
        const pulledVersion = createHash('sha256').update(await fs.readFile(sharedPath)).digest('hex'); assert.notEqual(pulledVersion, oldVersion);
        const deadline = Date.now() + 15_000;
        while (api.projectVersion() !== pulledVersion && Date.now() < deadline) await new Promise(resolve => setTimeout(resolve, 50));
        assert.equal(api.projectVersion(), pulledVersion, 'Git pull must trigger a real webview refresh without a manual search');
        assert.equal(p.page(await client.call({ op: 'search' })).total, 0);
        const status = (await git(project, ['status', '--porcelain', '--untracked-files=all'])).stdout;
        assert.ok(!status.includes('.shared.lock') && !status.includes('sqlite'));
        await fs.writeFile(path.join(qa, 'read.json'), JSON.stringify({ vscode: vscode.version, privateRecords: 0, sharedRecords: 3,
            sharedBindings: 2, localDocumentResolvedInClone: true, sourceResolvedInClone: true, copiedSharedSnapshot: true,
            gitPullAutomaticallyRendered: true, pluginDidNotCommit: true, runtimeLockIgnored: true }, null, 2));
    }
    await vscode.window.tabGroups.close(vscode.window.tabGroups.all.flatMap(group => group.tabs).filter(tab => tab.input instanceof vscode.TabInputWebview));
    console.log('Project sharing host phase passed: ' + phase);
}
