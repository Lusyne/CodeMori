import * as vscode from 'vscode';
import * as assert from 'node:assert/strict';
import * as fs from 'node:fs/promises';
import * as path from 'node:path';
import * as os from 'node:os';
import type { activate } from '../extension';

export async function run(): Promise<void> {
    const extension = vscode.extensions.getExtension('lusyne.codemori');
    assert.ok(extension);
    const api = await extension.activate() as Exclude<ReturnType<typeof activate>, undefined>;
    const output = process.env.CODEMORI_TEST_OUTPUT!;
    const fixture = JSON.parse(await fs.readFile(path.join(output, 'fixture.json'), 'utf8'));
    assert.equal(fixture.records, 10_000);
    assert.ok(fixture.searchable_text_bytes <= 50 * 1024 * 1024);
    const document = await vscode.workspace.openTextDocument(path.join(process.env.CODEMORI_TEST_PROJECT!, 'PaymentService.java'));
    await vscode.window.showTextDocument(document, vscode.ViewColumn.One);
    const opened = performance.now();
    await vscode.commands.executeCommand('codemori.open');
    await api.waitForRender();
    const panelReadyMs = performance.now() - opened;
    const measurements = [];
    for (const [query, expected] of [['redis 重试', 100], ['ordinary', 10_000], ['not-present-anywhere', 0]] as const) {
        const samples = [];
        for (let index = 0; index < 21; index++) {
            const sample = await api.measureSearch(query);
            assert.equal(sample.total, expected);
            assert.equal(sample.rows, Math.min(expected, 50));
            assert.equal(sample.visible, true, 'A hidden webview is not valid first-screen evidence');
            samples.push(sample.elapsedMs);
        }
        const warm = samples.slice(1).sort((a, b) => a - b);
        measurements.push({ query, matches: expected, first_query_ms: samples[0],
            median_ms: (warm[9] + warm[10]) / 2, p95_ms: warm[18], max_ms: warm[19], samples_ms: samples,
            first_target_met: samples[0] <= 1000, warm_target_met: warm[18] <= 300 });
    }
    const evidence = { fixture, vscode: vscode.version, platform: process.platform, arch: process.arch,
        os_release: os.release(), cpu: os.cpus()[0]?.model, memory_bytes: os.totalmem(), panel_ready_ms: panelReadyMs,
        scope: 'Visible webview search dispatch through production controller/native CLI and DOM render plus two animation frames; excludes typing debounce; OS caches not flushed; comment index empty.',
        measurements };
    await fs.writeFile(path.join(output, 'render-performance.json'), JSON.stringify(evidence, null, 2));
    console.log(JSON.stringify(evidence));
    assert.ok(measurements.every(item => item.first_target_met && item.warm_target_met), 'PRD A11 latency targets failed; see render-performance.json');
}
