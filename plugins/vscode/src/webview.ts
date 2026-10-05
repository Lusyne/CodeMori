import * as vscode from 'vscode';
import { randomBytes } from 'node:crypto';
import { readFileSync } from 'node:fs';
const escape = (value: string) => value.replace(/[&<>"']/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[c]!);
export function html(webview: vscode.Webview, extension: vscode.Uri): string {
    const nonce = randomBytes(16).toString('base64');
    const css = webview.asWebviewUri(vscode.Uri.joinPath(extension, 'media', 'app.css'));
    const script = webview.asWebviewUri(vscode.Uri.joinPath(extension, 'media', 'app.js'));
    return readFileSync(vscode.Uri.joinPath(extension, 'media', 'index.html').fsPath, 'utf8')
        .replaceAll('{{CSP}}', webview.cspSource).replaceAll('{{NONCE}}', nonce)
        .replace('{{CSS}}', css.toString()).replace('{{SCRIPT}}', script.toString());
}
import type { MarkdownPreview } from './protocol';

/** Only accepts the core's escaped renderer output, never document HTML. */
export function markdownHtml(preview: MarkdownPreview): string {
    return `<!doctype html><html lang="zh-CN"><head><meta charset="utf-8"><meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src 'unsafe-inline';"><title>只读预览</title><style>body{max-width:900px;margin:24px auto;padding:0 20px;color:var(--vscode-editor-foreground);background:var(--vscode-editor-background);font-family:var(--vscode-font-family);line-height:1.6;overflow-wrap:anywhere}pre{overflow:auto;padding:12px;background:var(--vscode-textCodeBlock-background)}table{border-collapse:collapse}td,th{border:1px solid var(--vscode-panel-border);padding:6px}header{opacity:.7;border-bottom:1px solid var(--vscode-panel-border)}</style></head><body><header><p>只读 · ${escape(preview.path)}<br>图片与链接跳转已禁用；预览正文不会加入搜索。</p></header><main>${preview.html}</main></body></html>`;
}

export function snippetHtml(title: string, content: string): string {
    return `<!doctype html><html lang="zh-CN"><head><meta charset="utf-8"><meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src 'unsafe-inline';"><title>${escape(title)}</title><style>body{padding:16px;color:var(--vscode-editor-foreground);background:var(--vscode-editor-background);font-family:var(--vscode-font-family)}header{color:var(--vscode-descriptionForeground);font-size:12px}pre{font-family:var(--vscode-editor-font-family,monospace);line-height:1.6;white-space:pre;overflow:auto}</style></head><body><header>只读代码快照 · ${escape(title)} · 保存时的内容，不会修改源文件</header><pre><code>${escape(content)}</code></pre></body></html>`;
}
