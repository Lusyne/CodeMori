import * as vscode from 'vscode';
import * as assert from 'node:assert/strict';
import * as fs from 'node:fs/promises';
import * as path from 'node:path';
import * as net from 'node:net';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { CoreClient, bundledExecutable } from '../core';
import * as p from '../protocol';
import type { activate } from '../extension';

let stage = 'starting';
export async function run(): Promise<void> {
    let timer: NodeJS.Timeout | undefined;
    try {
        await Promise.race([workflow(), new Promise<never>((_resolve, reject) => {
            timer = setTimeout(() => reject(new Error(`Packaged acceptance timed out at ${stage}`)), 45_000);
        })]);
    } finally { if (timer) clearTimeout(timer); }
}
async function workflow(): Promise<void> {
    const root = process.env.CODEMORI_PACKAGE_ROOT!;
    const phase = process.env.CODEMORI_PACKAGE_PHASE!;
    const extension = vscode.extensions.getExtension('lusyne.codemori'); assert.ok(extension);
    assert.equal(extension.extensionPath, process.env.CODEMORI_PACKAGE_PATH);
    stage = 'network probe';
    // The live server is reachable outside this sandbox; inside the actual host it must be denied.
    const networkError = await new Promise<NodeJS.ErrnoException | null>(resolve => {
        const socket = net.connect(Number(process.env.CODEMORI_PROBE_PORT), '127.0.0.1');
        socket.setTimeout(3000);
        socket.once('connect', () => { socket.destroy(); resolve(null); });
        socket.once('error', error => resolve(error));
        socket.once('timeout', () => { socket.destroy(); resolve(new Error('Timed out instead of explicit network denial')); });
    });
    if (process.env.CODEMORI_OFFLINE_TEST !== '0') assert.ok(networkError && ['EPERM', 'EACCES'].includes(networkError.code ?? ''), `Expected explicit offline denial, got ${networkError}`);
    else assert.equal(networkError, null, 'Control run should reach the live local probe');
    stage = 'toolchain and default home checks';
    await assert.rejects(promisify(execFile)('cargo', ['--version']), { code: 'ENOENT' });
    const binary = bundledExecutable(extension.extensionPath);
    const runtime = p.object(p.decode((await promisify(execFile)(binary, ['info'], {
        env: { ...process.env, HOME: path.join(root, 'home') }
    })).stdout));
    assert.equal(runtime.data_dir, path.join(root, 'home', '.codemori'));
    stage = 'extension activation';
    console.log('Packaged acceptance: native path, toolchain and network checks complete');
    const api = await extension.activate() as Exclude<ReturnType<typeof activate>, undefined>;
    assert.ok(api, 'Installed extension must be in Test mode for controller assertions');
    console.log('Packaged acceptance: extension activated');
    stage = 'opening source and panel';
    const sourcePath = path.join(root, 'project', 'PaymentService.java');
    const document = await vscode.workspace.openTextDocument(vscode.Uri.file(sourcePath));
    const editor = await vscode.window.showTextDocument(document, vscode.ViewColumn.One);
    await vscode.commands.executeCommand('codemori.open');
    await api.waitForRender();
    stage = `phase ${phase}`;
    const client = new CoreClient(binary, process.env.CODEMORI_TEST_DATA_DIR);
    const urls = ['https://example.feishu.cn/docx/codemori-qa', 'https://www.notion.so/codemori-qa', 'obsidian://open?vault=codemori-qa&file=design'];
    if (phase === 'write') {
        editor.selection = new vscode.Selection(1, 0, 3, 0);
        await vscode.commands.executeCommand('codemori.saveSelection');
        const draft = api.draft(); assert.ok(draft);
        const saved = await api.request({ id: 'package:save', action: 'save', token: draft.token,
            input: { ...draft.input, title: 'Offline packaged snapshot', tags: ['支付'] } });
        assert.equal(saved?.ok, true);
        for (const [index, url] of urls.entries()) {
            await vscode.commands.executeCommand('codemori.associateDocument');
            const form = api.draft(); assert.ok(form?.binding);
            const result = await api.request({ id: `package:link:${index}`, action: 'save', token: form.token,
                input: { ...form.input, title: `Linked note ${index}`, description: '离线可读的手填摘要', url } });
            assert.equal(result?.ok, true);
        }
        const record = p.record(saved.data);
        const priorClipboard = await vscode.env.clipboard.readText();
        try {
            assert.equal((await api.request({ id: 'package:copy', action: 'copy', recordId: record.id, revision: record.revision }))?.ok, true);
            assert.equal(await vscode.env.clipboard.readText(), draft.input.content);
        } finally {
            if (await vscode.env.clipboard.readText() === draft.input.content) await vscode.env.clipboard.writeText(priorClipboard);
        }
        await fs.writeFile(path.join(root, 'receipt.json'), JSON.stringify({ id: record.id, content: record.input.content }));
    } else {
        const receipt = JSON.parse(await fs.readFile(path.join(root, 'receipt.json'), 'utf8'));
        const read = p.record(await client.call({ op: 'record_get', id: receipt.id }));
        assert.equal(read.input.content, receipt.content);
        const result = await api.request({ id: 'package:restarted-search', action: 'search', filter: { query: '离线', kind: 'document' }, reloadTags: true });
        assert.equal(result?.ok, true);
        const state = p.object(result.data);
        assert.equal(p.page(state.result).total, 3);
        const documents = p.array(state.documents, p.record); assert.equal(documents.length, 3);
        for (const url of urls) assert.ok(documents.some(record => record.input.url === url));
        assert.ok(documents.every(record => record.input.description === '离线可读的手填摘要' && record.input.content === ''));
        const rendered = await api.measureSearch('离线');
        assert.equal(rendered.total, 3); assert.equal(rendered.rows, 3); assert.equal(rendered.visible, true);
        await fs.writeFile(path.join(root, 'evidence.json'), JSON.stringify({
            os: process.platform, arch: process.arch, vscode: vscode.version, installedVsix: true,
            nativeToolchainOnPath: false, dataOverride: true, cliDefaultHomeCheckedSeparately: true,
            processNetworkDenied: networkError?.code ?? false,
            sessions: 2, savedSelectedSnapshot: true, copiedSnapshot: true,
            copiedOffline: process.env.CODEMORI_OFFLINE_TEST !== '0', restoredBindingsAfterRestart: documents.length,
            visibleRenderedResultsAfterRestart: rendered.rows,
            scope: 'Installed VSIX files in real Extension Host Test mode; production controller invoked through Test-only seam. No manual desktop or external-app dispatch claim.'
        }));
    }
    await vscode.window.tabGroups.close(vscode.window.tabGroups.all.flatMap(group => group.tabs).filter(tab => tab.input instanceof vscode.TabInputWebview));
    console.log(`Packaged acceptance phase ${phase} (${process.env.CODEMORI_OFFLINE_TEST === '0' ? 'network control' : 'network denied'}): passed`);
}
