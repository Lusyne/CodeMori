import { spawn } from 'node:child_process';
import * as path from 'node:path';
import { CoreError, decode } from './protocol';

export function hostDirectory(platform: string = process.platform, arch: string = process.arch): string {
    const os = ({ darwin: 'macos', linux: 'linux', win32: 'windows' } as Record<string, string>)[platform];
    const cpu = ({ arm64: 'arm64', x64: 'x86_64' } as Record<string, string>)[arch];
    if (!os || !cpu) throw new CoreError('UNSUPPORTED_PLATFORM', `Unsupported platform: ${platform}/${arch}`);
    return `${os}-${cpu}`;
}
export function bundledExecutable(extensionPath: string): string {
    return path.join(extensionPath, 'bin', hostDirectory(), process.platform === 'win32' ? 'codemori.exe' : 'codemori');
}
export class CoreClient {
    constructor(readonly executable: string, readonly dataDirectory?: string) {}
    call(request: Record<string, unknown>, signal?: AbortSignal): Promise<unknown> {
        const payload = Buffer.from(JSON.stringify({ protocol_version: 1, request }), 'utf8');
        const max = 128 * 1024 * 1024;
        if (payload.length > max) return Promise.reject(new CoreError('TOO_LARGE', 'Request exceeds 128 MiB.'));
        if (signal?.aborted) return Promise.reject(new CoreError('CANCELLED', '操作已取消。'));
        return new Promise((resolve, reject) => {
            const args = ['rpc'];
            if (this.dataDirectory) args.push('--data-dir', this.dataDirectory);
            const child = spawn(this.executable, args, { shell: false, windowsHide: true, stdio: ['pipe', 'pipe', 'pipe'] });
            let output = 0; let errors = 0; let finished = false;
            const stdout: Buffer[] = []; const stderr: Buffer[] = [];
            const done = (error?: Error, value?: unknown) => {
                if (finished) return; finished = true; clearTimeout(timer); signal?.removeEventListener('abort', cancel);
                if (error) { if (child.exitCode === null && !child.killed) child.kill(); reject(error); } else resolve(value);
            };
            const cancel = () => { child.kill(); done(new CoreError('CANCELLED', '操作已取消。若刚才正在保存，请刷新确认结果。')); };
            const timer = setTimeout(() => { child.kill(); done(new CoreError('TIMEOUT', 'CodeMori 请求超时。请刷新后再重试。')); }, 30_000);
            signal?.addEventListener('abort', cancel, { once: true });
            child.on('error', error => done(new CoreError('PROCESS_ERROR', `无法启动随包核心程序：${error.message}`)));
            child.stdout.on('data', (chunk: Buffer) => {
                output += chunk.length;
                if (output > max) { child.kill(); done(new CoreError('TOO_LARGE', 'Response exceeds 128 MiB.')); }
                else stdout.push(chunk);
            });
            child.stderr.on('data', (chunk: Buffer) => { errors += chunk.length; if (errors < 65536) stderr.push(chunk); });
            child.stdin.on('error', error => { if (!finished) done(new CoreError('PROCESS_ERROR', `无法传递请求：${error.message}`)); });
            child.on('close', code => {
                if (finished) return;
                try {
                    const data = decode(Buffer.concat(stdout).toString('utf8'));
                    if (code !== 0) throw new CoreError('PROCESS_ERROR', `Core exited ${code}: ${Buffer.concat(stderr).toString('utf8')}`);
                    done(undefined, data);
                } catch (error) { done(error instanceof Error ? error : new Error(String(error))); }
            });
            child.stdin.end(payload);
        });
    }
}
