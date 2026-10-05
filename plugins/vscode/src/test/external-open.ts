import * as assert from 'node:assert/strict';
import * as fs from 'node:fs/promises';
import * as path from 'node:path';
import { createServer } from 'node:http';
import { randomUUID } from 'node:crypto';
import type { activate } from '../extension';
import { CoreClient } from '../core';
import * as p from '../protocol';

/** Opt-in OS browser dispatch through the actual document-opening controller. */
export async function verifyExternalOpen(api: Exclude<ReturnType<typeof activate>, undefined>, client: CoreClient): Promise<void> {
    const route = `/codemori-acceptance/${randomUUID()}/${encodeURIComponent('支付 设计')}?from=editor&tag=${encodeURIComponent('重试')}`;
    let received: (() => void) | undefined;
    const request = new Promise<void>(resolve => { received = resolve; });
    const server = createServer((req, res) => {
        if (req.url !== route) { res.writeHead(404); res.end(); return; }
        res.writeHead(200, { 'Content-Type': 'text/html; charset=utf-8', 'Cache-Control': 'no-store' });
        res.end('<!doctype html><html lang="zh"><meta charset="utf-8"><title>CodeMori 原文打开验收</title><h1>CodeMori 原文打开成功</h1><p>这是本机临时验收页面，不需要登录，也不包含个人资料。</p></html>');
        received?.();
    });
    await new Promise<void>((resolve, reject) => { server.once('error', reject); server.listen(0, '127.0.0.1', resolve); });
    let timer: NodeJS.Timeout | undefined;
    try {
        const address = server.address(); assert.ok(address && typeof address !== 'string');
        const url = `http://127.0.0.1:${address.port}${route}`;
        const record = p.record(await client.call({ op: 'record_create', record: { kind: 'document', title: 'Browser opening acceptance', url } }));
        const opened = await api.request({ id: 'host:external-open', action: 'open', recordId: record.id, revision: record.revision });
        assert.equal(opened?.ok, true, opened?.error);
        await Promise.race([request, new Promise<never>((_resolve, reject) => {
            timer = setTimeout(() => reject(new Error('Default browser did not request the exact saved URL')), 15_000);
        })]);
        const evidence = { url, exactEncodedPathAndQueryReceived: true, controllerReportedSuccess: true,
            scope: 'Real default-browser HTTP dispatch from installed VS Code using the production open-record controller in Test mode. Does not prove platform login or Obsidian URI handling.' };
        await fs.writeFile(path.join(process.env.CODEMORI_TEST_OUTPUT!, 'external-open.json'), JSON.stringify(evidence, null, 2));
        console.log(`CodeMori external-open passed: ${url}`);
    } finally {
        if (timer) clearTimeout(timer);
        server.closeAllConnections();
        await new Promise<void>((resolve, reject) => server.close(error => error ? reject(error) : resolve()));
    }
}
