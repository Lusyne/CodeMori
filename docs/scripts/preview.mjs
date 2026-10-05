import { createServer } from 'node:http'
import { readFile } from 'node:fs/promises'
import { resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import sirv from 'sirv'

export async function startPreview(directory, port = 4173) {
  const { base } = JSON.parse(await readFile(resolve(directory, 'site-meta.json'), 'utf8'))
  if (typeof base !== 'string' || !/^\/(?:[^?#]*\/)?$/.test(base) || base.includes('..')) {
    throw new Error('Invalid built site base; run npm run build first')
  }
  // dev:true makes sirv discover current files after rebuild, rather than caching
  // the initial filename list and returning 404 for newly hashed JS chunks.
  const serve = sirv(directory, { dev: true, etag: true, extensions: ['html'] })
  const server = createServer((req, res) => {
    res.setHeader('Cache-Control', 'no-cache')
    const path = (req.url || '/').split('?')[0]
    if ((path === '/' && base !== '/') || (base !== '/' && path === base.slice(0, -1))) {
      res.writeHead(302, { Location: base }); res.end(); return
    }
    const missing = () => { res.writeHead(404, { 'Content-Type': 'text/plain; charset=utf-8' }); res.end('Not found') }
    if (!path.startsWith(base)) { missing(); return }
    req.url = '/' + req.url.slice(base.length)
    serve(req, res, missing)
  })
  await new Promise((ok, fail) => { server.once('error', fail); server.listen(port, '127.0.0.1', ok) })
  return { server, base, url: `http://localhost:${server.address().port}${base}` }
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const args = process.argv.slice(2)
  if (args.length && (args.length !== 2 || args[0] !== '--port' || !/^\d+$/.test(args[1]))) {
    throw new Error('Usage: npm run preview -- [--port 4173]')
  }
  const { url } = await startPreview(fileURLToPath(new URL('../.vitepress/dist', import.meta.url)), Number(args[1] || 4173))
  console.log(`Documentation preview: ${url} (reads base from the build; rebuilds are supported)`)
}
