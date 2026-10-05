import { downloadAndUnzipVSCode, runTests, resolveCliArgsFromVSCodeExecutablePath } from '@vscode/test-electron';
import * as fs from 'node:fs/promises';
import * as path from 'node:path';
import { networkProbe } from './network-probe';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { bundledExecutable } from '../core';

const execute = promisify(execFile);
async function main(): Promise<void> {
    if (process.env.CI !== 'true' && process.env.CODEMORI_ALLOW_IDE_TEST !== '1') throw new Error('IDE launch disabled. Coordinate interactive acceptance before setting CODEMORI_ALLOW_IDE_TEST=1.');
    if (process.platform !== 'darwin') throw new Error('Offline packaged acceptance currently requires macOS sandbox-exec.');
    const extension = path.resolve(__dirname, '../..');
    const version = JSON.parse(await fs.readFile(path.join(extension, 'package.json'), 'utf8')).version;
    const parent = path.join(extension, '.vscode-test');
    await fs.mkdir(parent, { recursive: true });
    const root = await fs.mkdtemp(path.join(parent, 'package-acceptance-'));
    const project = path.join(root, 'project'); const home = path.join(root, 'home');
    for (const dir of [project, home]) await fs.mkdir(dir);
    await fs.writeFile(path.join(project, 'PaymentService.java'), 'class PaymentService {\n  // 支付重试\n  void retry() {}\n}\n');
    const binary = process.env.CODEMORI_VSCODE_EXECUTABLE || await downloadAndUnzipVSCode('1.96.0');
    const profile = ['--user-data-dir', path.join(root, 'user'), '--extensions-dir', path.join(root, 'extensions')];
    // Changing Electron's HOME can block macOS in default-Keychain creation. Isolate
    // only our store; the CLI's default-home resolution is checked separately.
    const env = { ...process.env, PATH: '/usr/bin:/bin:/usr/sbin:/sbin', CODEMORI_TEST_DATA_DIR: path.join(root, 'store') };
    const [cli, ...base] = resolveCliArgsFromVSCodeExecutablePath(binary);
    await execute(cli, [...base, ...profile, '--install-extension', path.join(extension, `codemori-${version}-darwin-${process.arch}.vsix`), '--force'], { env });
    const installed = (await fs.readdir(path.join(root, 'extensions'))).find(name => name.startsWith('lusyne.codemori-'));
    if (!installed) throw new Error('Installed extension directory missing');
    const installedPath = path.join(root, 'extensions', installed);
    await fs.access(bundledExecutable(installedPath));
    // VS Code maps test API calls by extension path; install only the external test driver here.
    const driver = path.join(installedPath, 'dist', 'test', 'packaged.js');
    await fs.mkdir(path.dirname(driver), { recursive: true });
    await fs.copyFile(path.join(__dirname, 'packaged.js'), driver);
    // Denial is process-scoped; never change the user's network interfaces or firewall.
    const profilePath = path.join(root, 'offline.sb');
    await fs.writeFile(profilePath, '(version 1) (allow default) (deny network-outbound (remote ip)) (deny network-inbound (local ip))');
    const wrapper = path.join(root, 'offline-code');
    const quote = (value: string) => "'" + value.replace(/'/g, "'\\''") + "'";
    await fs.writeFile(wrapper, `#!/bin/sh\nexec /usr/bin/sandbox-exec -f ${quote(profilePath)} ${quote(binary)} "$@"\n`, { mode: 0o755 });
    const probe = await networkProbe();
    const port = probe.port;
    try {
        for (const phase of ['write', 'restart']) {
            console.log(`Packaged acceptance ${phase}: ${root}`);
            await runTests({ vscodeExecutablePath: process.env.CODEMORI_OFFLINE_TEST === '0' ? binary : wrapper, extensionDevelopmentPath: installedPath,
                extensionTestsPath: driver, extensionTestsEnv: { ...env,
                    CODEMORI_PACKAGE_ROOT: root, CODEMORI_PACKAGE_PATH: installedPath, CODEMORI_PACKAGE_PHASE: phase, CODEMORI_PROBE_PORT: String(port), CODEMORI_OFFLINE_TEST: process.env.CODEMORI_OFFLINE_TEST },
                launchArgs: [project, ...profile, '--force-disable-user-env', '--skip-welcome', '--disable-workspace-trust', '--disable-extensions'] });
        }
        const result = JSON.parse(await fs.readFile(path.join(root, 'evidence.json'), 'utf8'));
        await fs.writeFile(path.join(parent, 'package-acceptance-result.json'), JSON.stringify(result, null, 2));
        console.log(JSON.stringify(result));
    } finally {
        await probe.close();
        await fs.rm(root, { recursive: true, force: true });
    }
}
main().catch(error => { console.error(error); process.exitCode = 1; });
