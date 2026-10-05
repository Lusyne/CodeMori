import { feishuOpenCommand } from '../feishu';
import test from 'node:test';
import assert from 'node:assert/strict';
import * as fs from 'node:fs/promises';
import * as path from 'node:path';
import * as os from 'node:os';
import { CoreClient, bundledExecutable, hostDirectory } from '../core';
import * as p from '../protocol';

const binary = process.env.CODEMORI_TEST_BINARY || bundledExecutable(path.resolve(__dirname, '../..'));

test('protocol rejects false success, unsupported versions and malformed records', () => {
    for (const value of ['not json', '{"protocol_version":2,"ok":true,"data":{}}',
        '{"protocol_version":1,"ok":"true","data":{}}', '{"protocol_version":1,"ok":true}']) {
        assert.throws(() => p.decode(value));
    }
    assert.throws(() => p.decode('{"protocol_version":1,"ok":false,"error":{"code":"CONFLICT","message":"changed"}}'),
        (error: unknown) => error instanceof p.CoreError && error.code === 'CONFLICT');
    assert.throws(() => p.record({ id: 'x', revision: 1, created_at: 0, updated_at: 0, is_demo: false, input: { kind: 'snippet' } }));
    assert.equal(hostDirectory('darwin', 'arm64'), 'macos-arm64');
    assert.equal(hostDirectory('win32', 'x64'), 'windows-x86_64');
    assert.throws(() => hostDirectory('unknown', 'x64'));
});

test('actual native client saves, searches, links and rejects stale edits', async () => {
    const root = await fs.mkdtemp(path.join(os.tmpdir(), 'codemori-node-'));
    try {
        const store = path.join(root, '中文 data'); const client = new CoreClient(binary, store);
        const workspace = p.workspace(await client.call({ op: 'workspace_register', root }));
        const record = p.record(await client.call({ op: 'record_create', record: {
            kind: 'snippet', title: 'Redis 支付重试', content: 'retryPayment();', tags: ['重试'],
            source: { workspace_id: workspace.id, path: 'Payment.java', line: 1 }
        }}));
        const doc = p.record(await client.call({ op: 'record_create', record: { kind: 'document', title: '设计文档', url: 'https://example.com/design', description: 'Redis 超时后重试' }}));
        assert.equal(await client.call({ op: 'document_target', id: doc.id, revision: doc.revision }), doc.input.url);
        await assert.rejects(client.call({ op: 'document_target', id: doc.id, revision: doc.revision + 1 }),
            (error: unknown) => error instanceof p.CoreError && error.code === 'CONFLICT');
        await assert.rejects(client.call({ op: 'record_create', record: { kind: 'document', title: '写入链接', url: 'obsidian://open?vault=demo&append=true' } }),
            (error: unknown) => error instanceof p.CoreError && error.code === 'VALIDATION_ERROR');
        await client.call({ op: 'document_link', binding: { workspace_id: workspace.id, path: 'Payment.java', document_id: doc.id } });
        const found = p.page(await client.call({ op: 'search', filter: { query: 'REDIS 重试', workspace_id: workspace.id } }));
        assert.equal(found.total, 2); assert.ok(found.items.some(hit => hit.excerpt.some(span => span.highlight)));
        const second = new CoreClient(binary, store);
        await second.call({ op: 'record_update', id: record.id, revision: record.revision, record: { ...record.input, starred: true } });
        await assert.rejects(client.call({ op: 'record_update', id: record.id, revision: record.revision, record: record.input }),
            (error: unknown) => error instanceof p.CoreError && error.code === 'CONFLICT');
        const backup = await client.call({ op: 'backup_export' });
        const invalid = JSON.parse(JSON.stringify(backup));
        invalid.records[0].revision = 0;
        invalid.records.push({ id: 'malformed' });
        const rejected = new CoreClient(binary, path.join(root, 'rejected'));
        const invalidPreview = p.report(await rejected.call({ op: 'backup_preview', backup: invalid }));
        assert.equal(invalidPreview.invalid_count, 2);
        assert.deepEqual(invalidPreview.invalid_entries.map(entry => entry.index).sort(), [0, 2]);
        await assert.rejects(rejected.call({ op: 'backup_import', backup: invalid }));
        assert.equal(p.page(await rejected.call({ op: 'search' })).total, 0);
        const restored = new CoreClient(binary, path.join(root, 'restored'));
        assert.equal(p.report(await restored.call({ op: 'backup_preview', backup })).new_records, 2);
        assert.equal(p.page(await restored.call({ op: 'search' })).total, 0);
        await restored.call({ op: 'backup_import', backup });
        const links = p.array(await restored.call({ op: 'file_documents', workspace_id: workspace.id, path: 'Payment.java' }), p.record);
        assert.equal(links[0].id, doc.id);
        assert.equal(p.record(await restored.call({ op: 'record_get', id: record.id })).input.starred, true);
    } finally { await fs.rm(root, { recursive: true, force: true }); }
});

test('argv/stdin transport preserves literal shell-like code and cancellation is not success', async () => {
    const root = await fs.mkdtemp(path.join(os.tmpdir(), 'codemori-node-'));
    try {
        const client = new CoreClient(binary, path.join(root, 'store'));
        const sentinel = path.join(root, 'must-not-exist');
        const code = `$(touch ${sentinel})\n中文 ' " \\`;
        const record = p.record(await client.call({ op: 'record_create', record: { kind: 'snippet', content: code } }));
        assert.equal(record.input.content, code);
        await assert.rejects(fs.access(sentinel));
        const controller = new AbortController(); controller.abort();
        await assert.rejects(client.call({ op: 'search' }, controller.signal), (error: unknown) => error instanceof p.CoreError && error.code === 'CANCELLED');
        await assert.rejects(new CoreClient(path.join(root, 'missing')).call({ op: 'search' }));
    } finally { await fs.rm(root, { recursive: true, force: true }); }
});

async function hangingClient(root: string): Promise<CoreClient> {
    const executable = path.join(root, 'hanging-cli');
    await fs.copyFile(path.resolve(__dirname, '../../../../tests/fixtures/hanging-cli.sh'), executable);
    await fs.chmod(executable, 0o700);
    return new CoreClient(executable, root);
}
async function childPid(root: string): Promise<number> {
    const deadline = Date.now() + 5000;
    while (Date.now() < deadline) {
        try {
            const pid = Number((await fs.readFile(path.join(root, 'child.pid'), 'utf8')).trim());
            if (Number.isInteger(pid) && pid > 0) return pid;
        } catch (error) { if ((error as NodeJS.ErrnoException).code !== 'ENOENT') throw error; }
        await new Promise(resolve => setTimeout(resolve, 10));
    }
    throw new Error('Controlled child did not start');
}
function alive(pid: number): boolean {
    try { process.kill(pid, 0); return true; }
    catch (error) { if ((error as NodeJS.ErrnoException).code === 'ESRCH') return false; throw error; }
}
async function assertStopped(pid: number): Promise<void> {
    const deadline = Date.now() + 2000;
    while (alive(pid) && Date.now() < deadline) await new Promise(resolve => setTimeout(resolve, 10));
    assert.equal(alive(pid), false, 'RPC child remained alive after failure');
}

test('a real unresponsive process times out and is terminated', { skip: process.platform === 'win32', timeout: 45_000 }, async () => {
    const root = await fs.mkdtemp(path.join(os.tmpdir(), 'codemori-timeout-'));
    let pid: number | undefined;
    try {
        const client = await hangingClient(root);
        const started = Date.now();
        await assert.rejects(client.call({ op: 'search' }), (error: unknown) => error instanceof p.CoreError && error.code === 'TIMEOUT');
        const elapsed = Date.now() - started;
        assert.ok(elapsed >= 29_000 && elapsed < 45_000, `Unexpected timeout duration: ${elapsed} ms`);
        pid = await childPid(root); await assertStopped(pid);
    } finally {
        if (pid && alive(pid)) process.kill(pid, 'SIGKILL');
        await fs.rm(root, { recursive: true, force: true });
    }
});

test('cancellation after process startup terminates the child and reports CANCELLED', { skip: process.platform === 'win32', timeout: 10_000 }, async () => {
    const root = await fs.mkdtemp(path.join(os.tmpdir(), 'codemori-cancel-'));
    const controller = new AbortController(); let pid: number | undefined;
    try {
        const client = await hangingClient(root);
        const rejected = assert.rejects(client.call({ op: 'search' }, controller.signal),
            (error: unknown) => error instanceof p.CoreError && error.code === 'CANCELLED');
        pid = await childPid(root); controller.abort();
        await rejected; await assertStopped(pid);
    } finally {
        controller.abort();
        if (pid && alive(pid)) process.kill(pid, 'SIGKILL');
        await fs.rm(root, { recursive: true, force: true });
    }
});


test('document target decoder accepts optional Feishu target and rejects missing fields', () => {
    assert.deepEqual(p.documentOpenTargets({ original_url: 'https://example.com/', feishu_applink: null }),
        { original_url: 'https://example.com/', feishu_applink: null });
    assert.throws(() => p.documentOpenTargets({ original_url: 'https://example.com/' }));
    assert.throws(() => p.documentOpenTargets({ original_url: 'https://example.com/', feishu_applink: true }));
});


test('Feishu OS dispatch preserves nested URL encoding as a single non-shell argument', () => {
    const original = 'https://tenant.feishu.cn/wiki/abc?a=1&title=中文%20空格#anchor';
    const target = 'feishu://applink.feishu.cn/client/web_url/open?mode=window&url=' + encodeURIComponent(original);
    for (const platform of ['darwin', 'win32', 'linux'] as NodeJS.Platform[]) {
        const [command, args] = feishuOpenCommand(target, platform);
        assert.ok(command); assert.equal(args.at(-1), target);
        assert.equal(new URL(args.at(-1)!).searchParams.get('url'), original);
    }
    for (const target of ['https://example.com/', 'feishu://evil.example/client/web_url/open',
        'feishu://applink.feishu.cn/client/op/open', 'feishu://user@applink.feishu.cn/client/web_url/open']) {
        assert.throws(() => feishuOpenCommand(target));
    }
});


test('first-use identity and an unmatched code link retain non-null RPC envelopes', async () => {
    const temp = await fs.mkdtemp(path.join(os.tmpdir(), 'codemori-identity-'));
    try {
        const store = path.join(temp, 'private'); const client = new CoreClient(binary, store);
        assert.equal(p.author(p.object(await client.call({ op: 'identity_get' })).author), null);
        await assert.rejects(fs.access(store));
        const configured = p.author(p.object(await client.call({ op: 'identity_set', display_name: 'Alice' })).author); assert.ok(configured);
        assert.equal(configured.display_name, 'Alice');
        await fs.writeFile(path.join(temp, 'main.rs'), 'fn main() {}');
        const links = p.object(await client.call({ op: 'code_link_create', root: temp, path: 'main.rs', line: 1 }));
        const other = path.join(temp, 'other'); await fs.mkdir(other);
        assert.equal(p.object(await client.call({ op: 'code_link_resolve', root: other, url: links.vscode_url })).target, null);
        const target = p.object(p.object(await client.call({ op: 'code_link_resolve', root: temp, url: links.vscode_url })).target);
        assert.equal(target.line, 1); assert.equal(await fs.realpath(p.string(target.path)), await fs.realpath(path.join(temp, 'main.rs')));
    } finally { await fs.rm(temp, { recursive: true, force: true }); }
});
