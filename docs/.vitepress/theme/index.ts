import { h } from 'vue'
import DefaultTheme from 'vitepress/theme'
import KnowledgePreview from './KnowledgePreview.vue'
import ReleaseDownloads from './ReleaseDownloads.vue'
import './custom.css'

export default {
  extends: DefaultTheme,
  enhanceApp({ app }) { app.component('ReleaseDownloads', ReleaseDownloads) },
  Layout: () => h(DefaultTheme.Layout, null, {
    'home-hero-image': () => h(KnowledgePreview)
  })
}
