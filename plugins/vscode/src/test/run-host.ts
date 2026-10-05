import { runTests, downloadAndUnzipVSCode } from '@vscode/test-electron';
import * as path from 'node:path';
import * as fs from 'node:fs/promises';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { bundledExecutable } from '../core';

async function main(): Promise<void> {
    if (process.env.CI !== 'true' && process.env.CODEMORI_ALLOW_IDE_TEST !== '1') throw new Error('IDE launch disabled. Coordinate interactive acceptance before setting CODEMORI_ALLOW_IDE_TEST=1.');
    const extension = path.resolve(__dirname, '../..');
    const benchmark = process.env.CODEMORI_BENCHMARK === '1';
    const shared = Boolean(process.env.CODEMORI_SHARED_RECORD_ID);
    const contextual = process.env.CODEMORI_CONTEXTUAL_TEST === '1';
    const projectSharing = Boolean(process.env.CODEMORI_PROJECT_SHARING_PHASE);
    if ([benchmark, shared, projectSharing, contextual].filter(Boolean).length > 1) throw new Error('Choose either the benchmark or the shared-IDE workflow.');
    const parent = path.join(extension, '.vscode-test');
    await fs.mkdir(parent, { recursive: true });
    const qa = await fs.mkdtemp(path.join(parent, benchmark ? 'render-benchmark-' : 'host-'));
    let project = path.join(qa, 'project');
    let store = path.join(qa, 'store');
    if (shared) {
        if (!process.env.CODEMORI_SHARED_PROJECT || !process.env.CODEMORI_SHARED_STORE) throw new Error('Shared IDE acceptance requires explicit project and store directories.');
        project = path.resolve(process.env.CODEMORI_SHARED_PROJECT);
        store = path.resolve(process.env.CODEMORI_SHARED_STORE);
        await fs.access(path.join(store, 'codemori.sqlite3'));
    }
    if (projectSharing) {
        const fixture = process.env.CODEMORI_PROJECT_SHARING_QA;
        if (!fixture) throw new Error('Project-sharing fixture missing');
        const root = await fs.realpath(fixture);
        if (!root.startsWith(await fs.realpath(parent) + path.sep) || !path.basename(root).startsWith('project-sharing-')) throw new Error('Project sharing tests require an owned temporary fixture');
        const phase = process.env.CODEMORI_PROJECT_SHARING_PHASE;
        if (phase !== 'write' && phase !== 'read') throw new Error('Invalid project sharing phase');
        project = path.join(root, phase === 'write' ? 'origin' : 'clone');
        store = path.join(root, phase === 'write' ? 'personal-a' : 'personal-b');
    }
    if (benchmark) {
        const prepared = await promisify(execFile)('python3', [path.resolve(extension, '../../scripts/benchmark-search.py'),
            bundledExecutable(extension), '--prepare', path.join(qa, 'fixture')], { timeout: 120_000 });
        const metadata = JSON.parse(prepared.stdout) as { store: string; project: string };
        store = metadata.store; project = metadata.project;
        await fs.writeFile(path.join(qa, 'fixture.json'), prepared.stdout);
    }
    await fs.mkdir(project, { recursive: true });
    if (!shared && !projectSharing) await fs.writeFile(path.join(project, 'PaymentService.java'), 'class PaymentService {\n  // 支付重试\n  void retry() {}\n}\n');
    let executable = process.env.CODEMORI_VSCODE_EXECUTABLE || await downloadAndUnzipVSCode(process.env.CODEMORI_VSCODE_VERSION || '1.140.0');
    try { await fs.access(executable); }
    catch (error) {
        if (process.platform !== 'darwin' || path.basename(executable) !== 'Electron') throw error;
        executable = path.join(path.dirname(executable), 'Code');
        await fs.access(executable);
    }
    console.log(`CodeMori acceptance directory: ${qa}`);
    await runTests({ vscodeExecutablePath: executable, extensionDevelopmentPath: extension, extensionTestsPath: path.join(__dirname, contextual ? 'contextual-host.js' : benchmark ? 'performance.js' : shared ? 'shared-host.js' : projectSharing ? 'project-sharing-host.js' : 'host.js'),
        extensionTestsEnv: { CODEMORI_TEST_DATA_DIR: store, CODEMORI_TEST_PROJECT: project, CODEMORI_TEST_OUTPUT: qa },
        launchArgs: [project, '--user-data-dir', path.join(qa, 'user'), '--extensions-dir', path.join(qa, 'extensions'), '--skip-welcome', '--disable-workspace-trust', '--disable-extensions'] });
}
main().catch(error => { console.error(error); process.exitCode = 1; });
