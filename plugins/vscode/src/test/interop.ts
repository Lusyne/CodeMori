import { CoreClient } from '../core';
import * as p from '../protocol';
import * as assert from 'node:assert/strict';

async function main(): Promise<void> {
    const [binary, data, id, phase = 'update'] = process.argv.slice(2);
    if (!binary || !data || !id) throw new Error('Expected binary, store and record ID');
    const client = new CoreClient(binary, data);
    if (phase === 'empty') {
        assert.equal(p.page(await client.call({ op: 'search' })).total, 0);
        assert.deepEqual(p.array(await client.call({ op: 'tags' }), p.string), []);
        process.stdout.write(JSON.stringify({ empty: true })); return;
    }
    const original = p.record(await client.call({ op: 'record_get', id }));
    if (phase === 'delete-and-create') {
        assert.equal(original.input.title, 'JetBrains 再次更新');
        assert.equal(original.input.content, 'updatedSnapshot();');
        assert.equal(original.revision, 3);
        const found = p.page(await client.call({ op: 'search', filter: { query: 'jEtBrAiNs 再次', tag: 'redis', starred: true } }));
        assert.equal(found.total, 1); assert.equal(found.items[0].record.id, id);
        await assert.rejects(client.call({ op: 'record_update', id, revision: 2, record: original.input }), { code: 'CONFLICT' });
        await client.call({ op: 'record_delete', id, revision: original.revision });
        await assert.rejects(client.call({ op: 'record_get', id }), { code: 'NOT_FOUND' });
        const created = p.record(await client.call({ op: 'record_create', record: { kind: 'snippet', title: 'VS Code 创建', content: 'newFromVSCode();', tags: ['新建'] } }));
        process.stdout.write(JSON.stringify(created)); return;
    }
    assert.equal(phase, 'update');
    if (original.input.title !== '来自 JetBrains') throw new Error('Wrong shared record');
    const updated = p.record(await client.call({ op: 'record_update', id, revision: original.revision,
        record: { ...original.input, title: 'VS Code 更新', starred: true, tags: ['跨编辑器', '中文'] } }));
    process.stdout.write(JSON.stringify(updated));
}
main().catch(error => { console.error(error); process.exitCode = 1; });
