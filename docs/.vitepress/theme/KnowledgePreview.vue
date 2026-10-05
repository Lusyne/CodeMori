<template>
  <div class="knowledge-preview" aria-label="CodeMori 使用场景示意，非产品截图">
    <div class="preview-bar"><span class="window-dots" aria-hidden="true">● ● ●</span><span>payment / retry.ts</span></div>
    <div class="preview-code"><div class="code-hint">↗ 2 篇关联文档 · 当前文件</div>
      <pre><code>
<span class="purple">export function</span> <span class="blue">shouldRetry</span>(error) {
  <span class="purple">return</span> error.type === <span class="green">'timeout'</span>
}</code></pre>
    </div>
    <div class="preview-knowledge"><div class="section-label"><span>项目共享</span></div>
      <div class="note-row"><div class="note-icon">文</div><div><strong>支付重试设计</strong><p>超时可以重试，业务拒绝不重试。</p></div><span class="note-arrow" aria-hidden="true">↗</span></div>
      <div class="note-meta"><span>飞书文档</span><span>由 Mori 保存</span></div>
      <div class="review-row"><span class="status-dot"></span>代码有变化，待复核</div>
    </div>
  </div>
</template>
