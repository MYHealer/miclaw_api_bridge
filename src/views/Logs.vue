<script setup lang="ts">
import { ref } from "vue";

const lines = ref<string[]>([
  "[info] log streaming will be wired to the proxy in M5.",
]);
</script>

<template>
  <h1>日志</h1>
  <p class="muted">实时显示代理收到的请求与响应（流式 chunk 计数）。M5 阶段接入。</p>
  <pre class="log">{{ lines.join("\n") }}</pre>
</template>
