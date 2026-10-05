import { readdir, readFile, stat } from 'node:fs/promises'
import { resolve, join } from 'node:path'

const output = resolve('.vitepress/dist')
const base = process.env.DOCS_BASE || '/'
const origin = 'https://docs.example.invalid'
const failures = []
async function walk(dir) {
  const entries = await readdir(dir, { withFileTypes: true })
  return (await Promise.all(entries.map(e => e.isDirectory() ? walk(join(dir, e.name)) : join(dir, e.name)))).flat()
}
const pages = (await walk(output)).filter(p => p.endsWith('.html'))
for (const file of pages) {
  const html = await readFile(file, 'utf8')
  const pageUrl = new URL(base + file.slice(output.length + 1), origin)
  for (const [, raw] of html.matchAll(/(?:href|src)="([^"<>]+)"/g)) {
    if (/^(?:data:|mailto:|tel:|javascript:)/.test(raw)) continue
    const url = new URL(raw.replaceAll('&amp;', '&'), pageUrl)
    if (url.origin !== origin) continue
    if (!url.pathname.startsWith(base)) { failures.push(`${file}: outside base ${raw}`); continue }
    let pathname = decodeURIComponent(url.pathname.slice(base.length))
    if (!pathname || pathname.endsWith('/')) pathname += 'index.html'
    const target = resolve(output, pathname)
    if (!target.startsWith(output + '/')) { failures.push(`${file}: unsafe target ${raw}`); continue }
    try {
      if (!(await stat(target)).isFile()) throw new Error('not a file')
      if (url.hash && target.endsWith('.html')) {
        const id = decodeURIComponent(url.hash.slice(1))
        const content = await readFile(target, 'utf8')
        if (!content.includes(`id="${id}"`)) failures.push(`${file}: missing anchor ${raw}`)
      }
    } catch { failures.push(`${file}: missing ${raw}`) }
  }
}
if (failures.length) { console.error([...new Set(failures)].join('\n')); process.exit(1) }
console.log(`Checked ${pages.length} HTML pages: local links, anchors and assets resolve under ${base}`)
