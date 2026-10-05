import { defineConfig } from 'vitepress'
import { writeFile } from 'node:fs/promises'

const base = process.env.DOCS_BASE || '/'
if (!base.startsWith('/') || !base.endsWith('/') || base.includes('..')) {
  throw new Error('DOCS_BASE must be an absolute URL path ending in /')
}
const repository = process.env.GITHUB_REPOSITORY || 'Lusyne/CodeMori'
const repo = repository && /^[\w.-]+\/[\w.-]+$/.test(repository)
  ? `https://github.com/${repository}` : undefined

export default defineConfig({
  lang: 'zh-CN', title: 'CodeMori',
  description: '代码在哪，文档就在哪。将代码与文档关联，把设计思路、团队经验、复用片段，嵌入开发工作流。',
  base, cleanUrls: false,
  srcExclude: ['node_modules/**'],
  head: [['link', { rel: 'icon', type: 'image/png', href: `${base}mark.png` }]],
  themeConfig: {
    logo: '/mark.png', siteTitle: 'CodeMori',
    nav: [
      { text: '使用指南', link: '/quickstart' },
      { text: '下载与安装', link: '/download' },
      { text: 'CLI 工具', link: '/cli' },
      { text: '更新日志', link: '/changelog' }
    ],
    socialLinks: repo ? [{ icon: 'github', link: repo }] : [],
    sidebar: [
      { text: '开始', items: [
        { text: '功能简介', link: '/overview' },
        { text: '插件下载与安装', link: '/download' },
        { text: 'CLI 安装与使用', link: '/cli' },
        { text: '快速开始', link: '/quickstart' },
        { text: '常见问题', link: '/faq' }
      ] },
      { text: '使用说明', items: [
        { text: 'VS Code', link: '/vscode' }, { text: 'JetBrains', link: '/jetbrains' },
        { text: '在飞书中打开', link: '/feishu-opening' },
        { text: '从文档回到代码', link: '/code-links' },
        { text: '注释索引与 Markdown', link: '/indexing' },
        { text: '团队共享与署名', link: '/project-sharing' },
        { text: '一键复核与 AI Skill', link: '/ai-skill' }
      ] },
      { text: '资料与版本', items: [
        { text: '数据存储与隐私', link: '/data-privacy' },
        { text: '更新日志', link: '/changelog' },
        { text: '兼容性与验证', link: '/compatibility' },
      ] },
      { text: '开发', collapsed: true, items: [
        { text: 'JSON RPC 协议', link: '/cli-protocol' },
      ] }
    ],
    search: { provider: 'local', options: { locales: { root: { translations: {
      button: { buttonText: '搜索文档', buttonAriaLabel: '搜索文档' },
      modal: { noResultsText: '没有找到相关内容', resetButtonTitle: '清空搜索',
        footer: { selectText: '选择', navigateText: '切换', closeText: '关闭' } }
    } } } } },
    outline: { label: '本页目录', level: [2, 3] },
    docFooter: { prev: '上一页', next: '下一页' },
    sidebarMenuLabel: '目录', returnToTopLabel: '返回顶部',
    darkModeSwitchLabel: '外观', lightModeSwitchTitle: '切换到浅色模式', darkModeSwitchTitle: '切换到深色模式',
  },
  async buildEnd(site) {
    await writeFile(`${site.outDir}/site-meta.json`, JSON.stringify({ base: site.site.base }))
  }
})
