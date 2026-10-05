const { version } = require('../package.json');
// Manual macOS acceptance using installed VSIX files and an isolated CodeMori store.
const { downloadAndUnzipVSCode, resolveCliArgsFromVSCodeExecutablePath } = require('@vscode/test-electron');
const { spawn } = require('node:child_process');
const fs = require('node:fs/promises');
const path = require('node:path');

async function run(binary, args, env) {
    await new Promise((resolve, reject) => {
        const child = spawn(binary, args, { env, stdio: 'inherit' });
        child.once('error', reject);
        child.once('exit', code => code === 0 ? resolve() : reject(new Error(`Process exited ${code}`)));
    });
}
(async () => {
    if (process.env.CODEMORI_ALLOW_IDE_TEST !== '1') throw new Error('IDE launch disabled. Coordinate interactive acceptance before setting CODEMORI_ALLOW_IDE_TEST=1.');
    if (process.platform !== 'darwin') throw new Error('This manual harness currently targets macOS only.');
    const extension = path.resolve(__dirname, '..');
    const parent = path.join(extension, '.vscode-test');
    await fs.mkdir(parent, { recursive: true });
    const root = await fs.mkdtemp(path.join(parent, 'packaged-manual-'));
    const project = path.join(root, 'project');
    const executable = process.env.CODEMORI_VSCODE_EXECUTABLE || await downloadAndUnzipVSCode('1.96.0');
    const [cli, ...cliArgs] = resolveCliArgsFromVSCodeExecutablePath(executable);
    for (const dir of [project, path.join(root, 'user', 'User')]) await fs.mkdir(dir, { recursive: true });
    const sample = path.join(project, 'PaymentService.java');
    try { await fs.access(sample); }
    catch { await fs.writeFile(sample, 'class PaymentService {\n  // 支付重试：复用幂等键\n  void retry() { sendPayment(); }\n}\n'); }
    await fs.writeFile(path.join(root, 'user', 'User', 'settings.json'), JSON.stringify({
        'telemetry.telemetryLevel': 'off', 'update.mode': 'none', 'extensions.autoUpdate': false,
        'extensions.autoCheckUpdates': false, 'workbench.startupEditor': 'none', 'window.restoreWindows': 'all',
        'security.workspace.trust.enabled': false
    }));
    const env = { ...process.env, PATH: '/usr/bin:/bin:/usr/sbin:/sbin', CODEMORI_TEST_DATA_DIR: path.join(root, 'store') };
    const profile = ['--user-data-dir', path.join(root, 'user'), '--extensions-dir', path.join(root, 'extensions')];
    await run(cli, [...cliArgs, ...profile, '--install-extension', path.join(extension, `codemori-${version}-darwin-${process.arch}.vsix`), '--force'], env);
    const installed = (await fs.readdir(path.join(root, 'extensions'))).find(name => name.startsWith('lusyne.codemori-'));
    if (!installed) throw new Error('Installed extension directory missing');
    console.log('Installed VSIX files, Development mode; isolated data:', env.CODEMORI_TEST_DATA_DIR);
    const child = spawn(executable, [project, '--new-window', ...profile, '--force-disable-user-env', '--skip-welcome',
        `--extensionDevelopmentPath=${path.join(root, 'extensions', installed)}`], { env, stdio: 'inherit' });
    console.log('QA PID:', child.pid);
    process.once('SIGTERM', () => child.kill());
    process.once('SIGINT', () => child.kill());
    child.once('error', error => { console.error(error); process.exitCode = 1; });
    child.once('exit', code => { process.exitCode = code || 0; });
})().catch(error => { console.error(error); process.exitCode = 1; });
