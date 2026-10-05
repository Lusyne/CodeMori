import test from 'node:test';
import assert from 'node:assert/strict';
import * as net from 'node:net';
import { networkProbe } from './network-probe';

test('acceptance probe survives an immediate TCP reset and remains reachable', async () => {
    const probe = await networkProbe();
    try {
        await new Promise<void>((resolve, reject) => {
            const socket = net.connect(probe.port, '127.0.0.1');
            socket.once('error', reject);
            socket.once('connect', () => socket.resetAndDestroy());
            socket.once('close', () => resolve());
        });
        const response = await new Promise<string>((resolve, reject) => {
            const socket = net.connect(probe.port, '127.0.0.1'); let text = '';
            socket.setEncoding('utf8'); socket.on('data', chunk => { text += chunk; });
            socket.once('error', reject); socket.once('end', () => resolve(text));
        });
        assert.equal(response, 'network probe');
    } finally { await probe.close(); }
});
