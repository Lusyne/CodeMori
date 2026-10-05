const { downloadAndUnzipVSCode } = require('@vscode/test-electron');
const { spawn } = require('node:child_process');
const fs = require('node:fs/promises');
const path = require('node:path');
(async () => {
    if (process.env.CODEMORI_ALLOW_IDE_TEST !== '1') throw new Error('IDE launch disabled. Coordinate interactive acceptance before setting CODEMORI_ALLOW_IDE_TEST=1.');
    const extension = path.resolve(__dirname, '..');
    const root = path.join(extension, '.vscode-test', 'manual-qa');
    const project = path.join(root, 'project');
    await fs.mkdir(project, {recursive:true});
    const sample = path.join(project,'PaymentService.java');
    try { await fs.access(sample); } catch { await fs.writeFile(sample,'class PaymentService {\n  // 支付重试：复用幂等键\n  void retry() { sendPayment(); }\n}\n'); }
    let executable = process.env.CODEMORI_VSCODE_EXECUTABLE || await downloadAndUnzipVSCode(process.env.CODEMORI_VSCODE_VERSION || '1.140.0');
    try { await fs.access(executable); } catch(error) {
        if(process.platform !== 'darwin' || path.basename(executable) !== 'Electron') throw error;
        executable=path.join(path.dirname(executable),'Code'); await fs.access(executable);
    }
    console.log('QA executable:', executable);
    console.log('QA data:', path.join(root,'store'));
    const child=spawn(executable,[project,'--new-window',`--extensionDevelopmentPath=${extension}`,
        '--user-data-dir',path.join(root,'user'),'--extensions-dir',path.join(root,'extensions'),
        '--skip-welcome','--disable-workspace-trust','--disable-extensions'],
        {stdio:'inherit',env:{...process.env,CODEMORI_TEST_DATA_DIR:path.join(root,'store')}});
    child.on('exit', code=>process.exitCode=code || 0);
    child.on('error', error=>{console.error(error);process.exitCode=1;});
})().catch(error=>{console.error(error);process.exitCode=1;});
