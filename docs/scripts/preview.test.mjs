import { test } from 'node:test'
import assert from 'node:assert/strict'
import { mkdtemp, mkdir, writeFile, rm } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { startPreview } from './preview.mjs'

for (const base of ['/', '/codemori/']) {
  test(`preview serves newly hashed assets after rebuild at ${base}`, async () => {
    const root = await mkdtemp(join(tmpdir(), 'codemori-docs-'))
    let server
    try {
      await mkdir(join(root, 'assets'))
      await writeFile(join(root, 'site-meta.json'), JSON.stringify({ base }))
      await writeFile(join(root, 'index.html'), '<h1>before</h1>')
      const running = await startPreview(root, 0); server = running.server
      assert.match(await (await fetch(running.url)).text(), /before/)
      assert.equal((await fetch(running.url + 'assets/new.js')).status, 404)
      await writeFile(join(root, 'assets/new.js'), 'export const ready = true')
      await writeFile(join(root, 'index.html'), `<script type="module" src="${base}assets/new.js"></script>`)
      const asset = await fetch(running.url + 'assets/new.js')
      assert.equal(asset.status, 200)
      assert.match(asset.headers.get('content-type'), /javascript/)
      assert.match(await asset.text(), /ready/)
      assert.match(await (await fetch(running.url)).text(), /new.js/)
      if (base !== '/') {
        const redirect = await fetch(new URL('/', running.url), { redirect: 'manual' })
        assert.equal(redirect.headers.get('location'), base)
      }
    } finally {
      if (server) await new Promise(resolve => server.close(resolve))
      await rm(root, { recursive: true, force: true })
    }
  })
}
