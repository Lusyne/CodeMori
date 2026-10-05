import { readdir, readFile } from 'node:fs/promises'
import { resolve, relative } from 'node:path'
import { startPreview } from './preview.mjs'

const root = resolve('.vitepress/dist')
async function files(path) {
  const entries = await readdir(path, { withFileTypes: true })
  return (await Promise.all(entries.map(e => e.isDirectory() ? files(resolve(path, e.name)) : resolve(path, e.name)))).flat()
}
const { server, url } = await startPreview(root, 0)
try {
  const assets = (await files(root)).filter(p => /\.(html|js|css|svg|json)$/.test(p))
  for (const path of assets) {
    const response = await fetch(url + relative(root, path).split('\\').join('/'))
    if (response.status !== 200) throw new Error(`HTTP ${response.status}: ${path}`)
    if (path.endsWith('.js') && !response.headers.get('content-type')?.includes('javascript')) throw new Error(`Wrong JS MIME: ${path}`)
    if (!(await readFile(path)).equals(Buffer.from(await response.arrayBuffer()))) throw new Error(`Stale HTTP content: ${path}`)
  }
  console.log(`HTTP checked ${assets.length} pages/assets at ${url}; bytes and JS MIME match build`)
} finally {
  await new Promise(resolve => server.close(resolve))
}
