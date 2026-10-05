import * as net from 'node:net';

/** Local acceptance fixture; clients deliberately disconnect to test outbound denial. */
export async function networkProbe(): Promise<{ port: number; close: () => Promise<void> }> {
    const sockets = new Set<net.Socket>();
    const server = net.createServer(socket => {
        sockets.add(socket);
        socket.once('close', () => sockets.delete(socket));
        socket.on('error', () => socket.destroy()); // A probe may reset immediately after connecting.
        socket.end('network probe');
    });
    await new Promise<void>((resolve, reject) => {
        server.once('error', reject);
        server.listen(0, '127.0.0.1', () => { server.removeListener('error', reject); resolve(); });
    });
    return { port: (server.address() as net.AddressInfo).port, close: () => {
        for (const socket of sockets) socket.destroy();
        return new Promise<void>((resolve, reject) => server.close(error => error ? reject(error) : resolve()));
    } };
}
