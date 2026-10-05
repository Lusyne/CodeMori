import test from 'node:test';
import assert from 'node:assert/strict';
import * as fs from 'node:fs';
import * as path from 'node:path';
import { JSDOM } from 'jsdom';

const media = path.resolve(__dirname, '../../media');
const script = fs.readFileSync(path.join(media, 'app.js'), 'utf8');
const markup = fs.readFileSync(path.join(media, 'index.html'), 'utf8');
interface Message { id: string; action: string; [key: string]: unknown }
function fixture() {
    const dom = new JSDOM(markup, { runScripts: 'outside-only', pretendToBeVisual: true, url: 'https://codemori.invalid' });
    const messages: Message[] = [];
    Object.assign(dom.window, { structuredClone, acquireVsCodeApi: () => ({ postMessage: (message: Message) => messages.push(message), getState: () => undefined, setState: () => {} }) });
    dom.window.eval(script);
    const reply = (message: Message, data: unknown, ok = true) => dom.window.dispatchEvent(new dom.window.MessageEvent('message', {
        data: { type: 'reply', id: message.id, ok, ...(ok ? { data } : { error: data }) }
    }));
    const notify = (data: unknown) => dom.window.dispatchEvent(new dom.window.MessageEvent('message', { data }));
    const byId = <T extends HTMLElement>(id: string) => dom.window.document.getElementById(id) as T;
    return { dom, messages, reply, notify, byId };
}
const record = { id: 'record-1', revision: 3, is_demo: false, created_at: 0, updated_at: 0, input: {
    kind: 'snippet', title: '<img id="injected" src="https://example.com">', content: '<script>alert(1)</script>',
    description: '说明', language: 'javascript', tags: ['中文'], starred: false, source: null, url: null
}};
const page = (title = record.input.title) => ({ result: { total: 1, limit: 50, offset: 0,
    items: [{ record: { ...record, input: { ...record.input, title } }, workspace_names: ['Demo'], excerpt: [{ text: '<script>中文</script>', highlight: true }] }] },
    documents: [], tags: ['中文'], filter: { query: '', tag: undefined }, file: null, workspaceId: null });
const tick = () => new Promise<void>(resolve => setImmediate(resolve));

test('render measurement uses the real search/render path and is disabled outside Test mode', async () => {
    const f = fixture();
    const wait = async (action: string) => {
        const deadline = Date.now() + 2000;
        while (!f.messages.some(m => m.action === action) && Date.now() < deadline) await new Promise(resolve => setTimeout(resolve, 10));
        const message = f.messages.find(m => m.action === action); assert.ok(message, `Missing ${action}`); return message;
    };
    try {
        f.notify({ type: 'testSearch', token: 'disabled', query: 'must-not-run' });
        assert.equal(f.messages.length, 1);
        f.reply(f.messages[0], { onboardingDismissed: true, testMode: true }); await tick();
        f.reply(f.messages.find(m => m.action === 'search')!, page());
        const ready = await wait('testReady'); f.reply(ready, {});
        f.notify({ type: 'testSearch', token: 'sample', query: '支付' });
        const query = f.messages.filter(m => m.action === 'search').at(-1)!;
        assert.equal((query.filter as {query: string}).query, '支付');
        f.reply(query, page());
        const measured = await wait('testMeasured');
        assert.equal(measured.token, 'sample'); assert.equal(measured.total, 1); assert.equal(measured.rows, 1);
        assert.equal(measured.visible, true); assert.ok(typeof measured.elapsedMs === 'number' && measured.elapsedMs >= 0);
    } finally { f.dom.window.close(); }
});

test('rendering escapes persisted content and mutations carry the observed revision', async () => {
    const f = fixture();
    try {
        f.reply(f.messages[0], { onboardingDismissed: true }); await tick();
        f.reply(f.messages.find(m => m.action === 'search')!, page()); await tick();
        assert.equal(f.dom.window.document.querySelector('#injected'), null);
        assert.match(f.byId('preview-code').textContent!, /<script>/);
        assert.equal(f.dom.window.document.querySelector('mark')!.textContent, '<script>中文</script>');
        f.byId<HTMLButtonElement>('favorite').click();
        const request = f.messages.at(-1)!;
        assert.equal(request.action, 'favorite'); assert.equal(request.revision, 3); assert.equal(request.recordId, 'record-1');
    } finally { f.dom.window.close(); }
});

test('third-source results escape comments and navigate with indexed source metadata', async () => {
    const f = fixture();
    try {
        f.reply(f.messages[0], { onboardingDismissed: true }); await tick();
        const source = { workspace_id: 'project-1', path: 'main.py', line: 7 };
        f.reply(f.messages.find(m => m.action === 'search')!, { ...page(), comments: { total: 1, items: [{ source, text: '# <img id="evil">',
            workspace_name: 'Project', language: 'python', indexed_at: 1000, excerpt: [{ text: '<img id="evil">', highlight: true }] }] },
            indexStatus: { files: 1, comments: 1, indexed_at: 1000 } });
        await tick();
        assert.equal(f.dom.window.document.querySelector('#evil'), null);
        assert.match(f.byId('comments').textContent!, /main.py:7/);
        f.byId('comments').querySelector('button')!.click();
        const request = f.messages.at(-1)!; assert.equal(request.action, 'openComment');
        assert.deepEqual(JSON.parse(JSON.stringify(request.source)), source);
    } finally { f.dom.window.close(); }
});

test('current-file documents expose source and summary immediately, with escaped content', async () => {
    const f = fixture();
    try {
        f.reply(f.messages[0], { onboardingDismissed: true }); await tick();
        const doc = { ...record, input: { ...record.input, kind: 'document', title: '支付设计', content: '',
            description: '超时可重试；<img id="injected-summary">业务拒绝不重试', url: 'https://example.com/design' } };
        f.reply(f.messages.find(m => m.action === 'search')!, { ...page(), documents: [doc], file: 'Payment.java', workspaceId: 'project' });
        await tick();
        const card = f.byId('documents').querySelector<HTMLButtonElement>('.document-link')!;
        assert.match(card.textContent!, /支付设计/); assert.match(card.textContent!, /来源：https:\/\/example.com\/design/);
        assert.match(card.textContent!, /业务拒绝不重试/); assert.equal(f.dom.window.document.querySelector('#injected-summary'), null);
        card.click();
        assert.equal(f.byId('preview-title').textContent, '支付设计');
        assert.equal(f.byId<HTMLButtonElement>('unlink').disabled, false);
    } finally { f.dom.window.close(); }
});

test('empty results report the effective scope and literal tag rather than hiding filters', async () => {
    const f = fixture();
    try {
        f.reply(f.messages[0], { onboardingDismissed: true }); await tick();
        f.reply(f.messages.find(m => m.action === 'search')!, { ...page(), result: { items: [], total: 0, offset: 0, limit: 50 },
            filter: { kind: 'snippet', workspace_id: 'project', tag: '<html>支付', starred: true, demo: false } });
        await tick();
        assert.equal(f.byId('empty').hidden, false);
        assert.match(f.byId('empty').textContent!, /个人 \+ 当前项目共享 · 当前项目 · 代码片段 · 标签：<html>支付 · 仅个人收藏/);
        assert.equal(f.byId('empty').querySelector('html'), null);
    } finally { f.dom.window.close(); }
});

test('save failure preserves form values and unlocks the editor', async () => {
    const f = fixture();
    try {
        f.notify({ type: 'edit', token: 'draft-1', title: '编辑片段', input: record.input, tags: [] });
        f.byId<HTMLInputElement>('title').value = '不能丢的草稿';
        f.byId<HTMLTextAreaElement>('content').value = 'newCode();';
        f.byId<HTMLFormElement>('record-form').dispatchEvent(new f.dom.window.Event('submit', { bubbles: true, cancelable: true }));
        assert.equal(f.byId<HTMLFieldSetElement>('fields').disabled, true);
        const request = f.messages.at(-1)!; assert.equal(request.action, 'save'); assert.equal(request.token, 'draft-1');
        f.reply(request, 'CONFLICT: refresh first', false); await tick();
        assert.equal(f.byId<HTMLInputElement>('title').value, '不能丢的草稿');
        assert.equal(f.byId<HTMLTextAreaElement>('content').value, 'newCode();');
        assert.equal(f.byId<HTMLFieldSetElement>('fields').disabled, false);
        assert.equal(f.byId('editor').hidden, false);
        assert.match(f.byId('editor-error').textContent!, /CONFLICT/);
    } finally { f.dom.window.close(); }
});

test('late searches cannot replace newer results and contexts clear stale bindings', async () => {
    const f = fixture();
    try {
        f.reply(f.messages[0], { onboardingDismissed: true }); await tick();
        const old = f.messages.find(m => m.action === 'search')!;
        f.byId<HTMLInputElement>('query').value = 'new'; f.byId<HTMLButtonElement>('search').click();
        const fresh = f.messages.at(-1)!;
        f.reply(fresh, page('new result')); await tick();
        f.reply(old, page('old result')); await tick();
        assert.equal(f.byId('preview-title').textContent, 'new result');
        f.notify({ type: 'contextChanged' });
        assert.equal(f.byId<HTMLButtonElement>('unlink').disabled, true);
        assert.equal(f.byId('documents').children.length, 0);
    } finally { f.dom.window.close(); }
});

test('CSP and local asset template do not allow remote or inline code', () => {
    assert.match(markup, /default-src 'none'/);
    assert.match(markup, /script-src 'nonce-\{\{NONCE\}\}'/);
    assert.equal(/on(click|load)=/.test(markup), false);
    assert.equal(/innerHTML\s*=/.test(script), false);
});


test('Feishu entry uses core capabilities and keeps the original browser action', async () => {
    const f = fixture();
    try {
        f.reply(f.messages[0], { onboardingDismissed: true }); await tick();
        const doc = { ...record, input: { ...record.input, kind: 'document', url: 'https://tenant.feishu.cn/wiki/abc' } };
        const base = page();
        const data = { ...base, result: { ...base.result, items: [{ ...base.result.items[0], record: doc }] } };
        f.reply(f.messages.find(m => m.action === 'search')!, data); await tick();
        const targets = f.messages.find(m => m.action === 'documentTargets')!;
        assert.equal(targets.recordId, doc.id); assert.equal(targets.revision, doc.revision);
        assert.equal(f.byId('openFeishu').hidden, true);
        f.reply(targets, { original_url: doc.input.url, feishu_applink: 'feishu://applink.feishu.cn/client/web_url/open?mode=window&url=encoded' }); await tick();
        assert.equal(f.byId('openFeishu').hidden, false);
        assert.equal(f.byId('open').textContent, '浏览器打开');
        f.byId<HTMLButtonElement>('openFeishu').click();
        const launch = f.messages.at(-1)!;
        assert.equal(launch.action, 'openFeishu'); assert.equal(launch.recordId, doc.id); assert.equal(launch.revision, doc.revision);
        assert.equal(launch.url, undefined); // The host re-resolves the observed record instead of trusting a webview URL.
        f.reply(launch, {}); await tick();
        assert.match(f.byId('status').textContent!, /已请求飞书打开/);
        f.byId<HTMLButtonElement>('open').click(); assert.equal(f.messages.at(-1)!.action, 'open');
        assert.equal(f.byId('preview-code').textContent, doc.input.url);
    } finally { f.dom.window.close(); }
});

test('late Feishu capabilities cannot restore an action for another selection', async () => {
    const f = fixture();
    try {
        f.reply(f.messages[0], { onboardingDismissed: true }); await tick();
        const doc = { ...record, id: 'doc', input: { ...record.input, kind: 'document', url: 'https://tenant.feishu.cn/docx/abc' } };
        f.reply(f.messages.find(m => m.action === 'search')!, { ...page(), documents: [doc], file: 'Payment.java', workspaceId: 'project' }); await tick();
        f.byId('documents').querySelector<HTMLButtonElement>('button')!.click();
        const targets = f.messages.at(-1)!; assert.equal(targets.action, 'documentTargets');
        f.byId('results').querySelector<HTMLButtonElement>('button')!.click();
        f.reply(targets, { original_url: doc.input.url, feishu_applink: 'feishu://applink.feishu.cn/client/web_url/open' }); await tick();
        assert.equal(f.byId('openFeishu').hidden, true); assert.equal(f.byId('open').textContent, '打开来源 / 原文');
        f.byId('documents').querySelector<HTMLButtonElement>('button')!.click();
        f.reply(f.messages.at(-1)!, { original_url: 'https://notion.so/page', feishu_applink: null }); await tick();
        assert.equal(f.byId('openFeishu').hidden, true);
    } finally { f.dom.window.close(); }
});


test('project provenance, composite selection and explicit save scope remain separate from personal data', async () => {
    const f = fixture();
    try {
        f.reply(f.messages[0], { onboardingDismissed: true }); await tick();
        const shared = { ...record, scope: 'project', project_root: '/repo', project_version: 'hash-one', input: { ...record.input, title: 'Shared same ID' } };
        const base = page('Personal same ID');
        f.reply(f.messages.find(m => m.action === 'search')!, { ...base, project: { root: '/repo', exists: true, error: null, version: 'hash-one' },
            result: { ...base.result, total: 2, items: [...base.result.items, { ...base.result.items[0], record: shared }] } }); await tick();
        const buttons = f.byId('results').querySelectorAll<HTMLButtonElement>('button');
        buttons[1].click();
        assert.match(f.byId('preview-kind').textContent!, /项目共享/); assert.equal(f.byId('favorite').hidden, true);
        assert.equal(buttons[0].getAttribute('aria-selected'), 'false'); assert.equal(buttons[1].getAttribute('aria-selected'), 'true');
        f.byId<HTMLButtonElement>('copy').click();
        const copy = f.messages.at(-1)!; assert.equal(copy.scope, 'project'); assert.equal(copy.projectRoot, '/repo'); assert.equal(copy.projectVersion, 'hash-one');
        f.notify({ type: 'edit', token: 'new', title: 'New', input: record.input, scope: 'personal', scopeLocked: false,
            project: { root: '/repo', version: 'hash-one', exists: true, error: null } });
        const scope = f.byId<HTMLSelectElement>('saveScope'); assert.equal(scope.value, 'personal'); assert.equal(scope.disabled, false);
        scope.value = 'project'; f.byId<HTMLFormElement>('record-form').dispatchEvent(new f.dom.window.Event('submit', { cancelable: true }));
        assert.equal(f.messages.at(-1)!.scope, 'project');
        f.reply(f.messages.at(-1)!, 'Project shared file changed', false); await tick();
        assert.equal(scope.value, 'project'); assert.equal(f.byId<HTMLInputElement>('title').value, record.input.title);
        f.notify({ type: 'edit', token: 'existing', title: 'Edit shared', input: shared.input, scope: 'project', scopeLocked: true,
            project: { root: '/repo', version: 'hash-one', exists: true, error: null } });
        assert.equal(scope.value, 'project'); assert.equal(scope.disabled, true);
    } finally { f.dom.window.close(); }
});

test('invalid shared files show a warning while personal results and creation stay available', async () => {
    const f = fixture();
    try {
        f.reply(f.messages[0], { onboardingDismissed: true }); await tick();
        f.reply(f.messages.find(m => m.action === 'search')!, { ...page('Personal survives'), project: { exists: true, error: 'Merge conflict', version: 'unavailable' } }); await tick();
        assert.equal(f.byId('project-status').hidden, false); assert.match(f.byId('project-status').textContent!, /个人资料仍可使用/);
        assert.equal(f.byId('preview-title').textContent, 'Personal survives');
        f.notify({ type: 'edit', token: 'new', title: 'New', input: record.input, project: { error: 'Merge conflict' } });
        assert.equal(f.byId<HTMLSelectElement>('saveScope').value, 'personal');
        assert.equal(f.byId('saveScope').querySelector<HTMLOptionElement>('[value="project"]')!.disabled, true);
    } finally { f.dom.window.close(); }
});


test('saving a new shared record clears personal-only filters so the saved row is visible', async () => {
    const f = fixture();
    try {
        f.reply(f.messages[0], { onboardingDismissed: true }); await tick();
        f.reply(f.messages.find(m => m.action === 'search')!, page()); await tick();
        f.byId<HTMLInputElement>('query').value = 'unrelated';
        f.byId<HTMLInputElement>('starred').checked = true;
        f.byId<HTMLSelectElement>('kind').value = 'document';
        f.notify({ type: 'edit', token: 'new', title: 'New', input: record.input, scope: 'personal', scopeLocked: false,
            project: { root: '/repo', version: 'missing', exists: false, error: null } });
        f.byId<HTMLSelectElement>('saveScope').value = 'project';
        f.byId<HTMLFormElement>('record-form').dispatchEvent(new f.dom.window.Event('submit', { cancelable: true }));
        const shared = { ...record, scope: 'project', project_root: '/repo', project_version: 'created' };
        f.reply(f.messages.at(-1)!, shared); await tick();
        const search = f.messages.at(-1)!;
        assert.equal(search.action, 'search'); assert.equal(search.libraryScope, 'project');
        assert.deepEqual(JSON.parse(JSON.stringify(search.filter)), { query: '', kind: '', tag: '', starred: false, demo: false, limit: 50, offset: 0 });
        assert.equal(f.byId('library').hidden, false);
        f.reply(search, { ...page(), result: { ...page().result, items: [{ ...page().result.items[0], record: shared }] } }); await tick();
        assert.equal(f.byId('results').children.length, 1);
        assert.match(f.byId('status').textContent!, /已保存到项目共享/);
    } finally { f.dom.window.close(); }
});


test('shared attribution is visible and escaped, and code-link commands use the narrow host actions', async () => {
    const f = fixture();
    try {
        f.reply(f.messages[0], { onboardingDismissed: true }); await tick();
        const shared = { ...record, scope: 'project', project_root: '/repo', project_version: 'v2',
            created_by: { id: 'creator', display_name: '<img id="author-injected">' }, updated_by: { id: 'editor', display_name: 'Bob' } };
        f.reply(f.messages.find(m => m.action === 'search')!, { ...page(), result: { ...page().result, items: [{ ...page().result.items[0], record: shared }] } }); await tick();
        assert.match(f.byId('preview-authors').textContent!, /创建者：<img/); assert.match(f.byId('preview-authors').textContent!, /最近修改：Bob/);
        assert.equal(f.dom.window.document.getElementById('author-injected'), null);
        for (const action of ['copyCodeLink', 'openCodeLink', 'identity']) {
            f.byId<HTMLButtonElement>(action).click(); assert.equal(f.messages.at(-1)!.action, action);
            f.reply(f.messages.at(-1)!, {}); await tick();
        }
    } finally { f.dom.window.close(); }
});


test('inherited associations retain their real module binding and review action payload', async () => {
    const f=fixture();
    try {
        f.reply(f.messages[0], {onboardingDismissed:true}); await tick();
        const doc={...record,input:{...record.input,kind:'document',url:'https://example.com'}};
        const binding={workspace_id:'w',path:'pay',document_id:doc.id,kind:'module',review:{fingerprint:'old',confirmed_at:1,confirmed_by:{display_name:'Alice'}}};
        f.reply(f.messages.find(m=>m.action==='search')!,{...page(),documents:[doc],file:'pay/a.ts',workspaceId:'w',associations:[{record:doc,binding,inherited:true,review_state:{status:'needs_review',fingerprint:'current',error:null}}]});await tick();
        assert.match(f.byId('documents').textContent!,/继承自目录：pay/);f.byId('documents').querySelector<HTMLButtonElement>('button')!.click();
        assert.match(f.byId('preview-review').textContent!,/待复核/);assert.match(f.byId('preview-review').textContent!,/Alice/);
        f.byId<HTMLButtonElement>('review').click();assert.equal(f.messages.at(-1)!.fingerprint,'current');assert.equal(JSON.parse(JSON.stringify(f.messages.at(-1)!.binding)).path,'pay');
        f.byId<HTMLButtonElement>('unlink').click();assert.equal(JSON.parse(JSON.stringify(f.messages.at(-1)!.binding)).kind,'module');
        for(const action of ['toggleHints','repairPaths']){f.byId<HTMLButtonElement>(action).click();assert.equal(f.messages.at(-1)!.action,action);}
    } finally {f.dom.window.close();}
});


test('one-click current-file review sends only the displayed context token and reports partial outcomes', async () => {
    const f=fixture();
    try {
        f.reply(f.messages[0],{onboardingDismissed:true});await tick();
        const entry={record:{...record,scope:'personal'},binding:{workspace_id:'w',path:'module',document_id:record.id,kind:'module'},inherited:true,review_state:{status:'needs_review',fingerprint:'observed'}};
        f.reply(f.messages.find(m=>m.action==='search')!,{...page(),reviewToken:'snapshot-1',associations:[entry]});await tick();
        assert.equal(f.byId<HTMLButtonElement>('reviewCurrent').disabled,false);assert.match(f.byId('review-scope').textContent!,/含 1 项/);
        f.byId<HTMLButtonElement>('reviewCurrent').click();const request=f.messages.at(-1)!;assert.equal(request.action,'reviewCurrent');assert.equal(request.token,'snapshot-1');assert.equal(request.binding,undefined);assert.equal(f.byId<HTMLButtonElement>('reviewCurrent').disabled,true);
        f.reply(request,{file:'repo/a.ts',confirmed:0,errors:['项目共享：已变更']});await tick();
        const refresh=f.messages.at(-1)!;assert.equal(refresh.action,'search');f.reply(refresh,{...page(),reviewToken:'snapshot-2',associations:[entry]});await tick();
        assert.match(f.byId('batch-review-result').textContent!,/未完成/);assert.match(f.byId('batch-review-result').textContent!,/已确认 0 项/);assert.equal(f.byId<HTMLButtonElement>('reviewCurrent').disabled,false);
    } finally { f.dom.window.close(); }
});

test('sidebar snapshot action carries observed identity and demo controls are absent', async () => {
    const f = fixture();
    try {
        f.reply(f.messages[0], { onboardingDismissed: true }); await tick();
        f.reply(f.messages.find(m => m.action === 'search')!, page()); await tick();
        for (const id of ['demo', 'demoInstall', 'demoRemove']) assert.equal(f.dom.window.document.getElementById(id), null);
        assert.equal(f.byId('previewSnippet').hidden, false);
        f.byId<HTMLButtonElement>('previewSnippet').click();
        const sent = f.messages.at(-1)!;
        assert.equal(sent.action, 'previewSnippet'); assert.equal(sent.recordId, record.id); assert.equal(sent.revision, record.revision);
        f.reply(sent, {});
        f.notify({ type: 'contextChanged' });
        const search = f.messages.at(-1)!;
        f.reply(search, { ...page(), result: { ...page().result, items: [{ ...page().result.items[0], record: { ...record, input: { ...record.input, kind: 'document', content: '', url: 'https://example.com' } } }] } });
        await tick();
        assert.equal(f.byId('previewSnippet').hidden, true);
    } finally { f.dom.window.close(); }
});
