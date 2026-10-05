<script setup>
import { computed } from 'vue'
import { useData } from 'vitepress'
const { theme } = useData()
const repository = computed(() => theme.value.socialLinks?.find(item => item.icon === 'github')?.link)
</script>

<template>
  <p v-if="repository"><a :href="`${repository}/releases`" target="_blank" rel="noopener noreferrer">前往 GitHub Releases 下载 →</a></p>
  <p v-else>本地预览尚未关联 GitHub 仓库；推送后的文档构建会显示该仓库的 Releases 入口。</p>
</template>
