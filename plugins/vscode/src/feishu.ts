import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import * as path from 'node:path';
import { CoreError } from './protocol';

/** OS routing only: Rust has already resolved the saved record and document URL. */
export function feishuOpenCommand(target: string, platform = process.platform): [string, string[]] {
    const uri = new URL(target);
    if (uri.protocol !== 'feishu:' || uri.host !== 'applink.feishu.cn'
        || uri.pathname !== '/client/web_url/open' || uri.username || uri.password || uri.hash) {
        throw new CoreError('INVALID_RESPONSE', '核心返回了不支持的飞书入口，请更新插件。');
    }
    switch (platform) {
        case 'darwin': return ['/usr/bin/open', [target]];
        case 'win32': return [path.win32.join(process.env.SystemRoot || 'C:\\Windows', 'System32', 'rundll32.exe'), ['url.dll,FileProtocolHandler', target]];
        case 'linux': return ['xdg-open', [target]];
        default: throw new CoreError('OPEN_FAILED', '此系统暂不支持唤起飞书，请使用浏览器打开。');
    }
}

export async function openFeishuAppLink(target: string): Promise<void> {
    const [command, args] = feishuOpenCommand(target);
    try {
        // env.openExternal(Uri.parse(...)) rewrites nested query escaping. Pass the
        // complete core-produced URI as one argument, without a command shell.
        await promisify(execFile)(command, args, { windowsHide: true, timeout: 10_000 });
    } catch {
        throw new CoreError('OPEN_FAILED', '未能唤起飞书，请确认已安装客户端，或点击“浏览器打开”。');
    }
}
