<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from "vue";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

interface LogRow {
  ts: number;
  kind: "request" | "response" | "error" | string;
  path?: string;
  model?: string;
  stream?: boolean;
  status?: number;
  elapsed_ms?: number;
  message?: string;
}

const rows = ref<LogRow[]>([]);
const max = 500;
let unlisten: UnlistenFn | null = null;

onMounted(async () => {
  unlisten = await listen<LogRow>("proxy-log", (e) => {
    rows.value.unshift(e.payload);
    if (rows.value.length > max) rows.value.length = max;
  });
});

onBeforeUnmount(() => {
  if (unlisten) unlisten();
});

function fmtTime(ts: number) {
  return new Date(ts).toLocaleTimeString();
}

function tagClass(kind: string, status?: number) {
  if (kind === "error") return "bad";
  if (kind === "response") {
    if (status && status >= 400) return "bad";
    if (status && status >= 200 && status < 300) return "ok";
    return "warn";
  }
  return "warn";
}

function clear() {
  rows.value = [];
}
</script>

<template>
  <h1>日志</h1>
  <p class="muted">
    实时显示本地代理收到的请求与响应（不记录任何请求体或响应体内容）。
  </p>

  <section class="card">
    <div class="row">
      <span class="tag warn">{{ rows.length }} 条</span>
      <div class="grow"></div>
      <button class="ghost" @click="clear">清空</button>
    </div>
  </section>

  <section class="card" v-if="rows.length === 0">
    <p class="muted">还没有请求。启动代理并用 OpenAI / Anthropic 客户端连接 :8765 试试。</p>
  </section>

  <section class="card" v-for="(r, i) in rows" :key="i">
    <div class="row">
      <span class="tag" :class="tagClass(r.kind, r.status)">
        {{ r.kind === "response" ? `↩ ${r.status ?? ""}` : r.kind === "request" ? "→" : "✗" }}
      </span>
      <code class="grow">{{ r.path }}</code>
      <span class="muted">{{ fmtTime(r.ts) }}</span>
    </div>
    <div class="row" v-if="r.kind === 'request'">
      <span class="muted">model={{ r.model || "—" }} stream={{ r.stream ? "true" : "false" }}</span>
    </div>
    <div class="row" v-if="r.kind === 'response'">
      <span class="muted">耗时 {{ r.elapsed_ms ?? "—" }}ms</span>
    </div>
    <div class="row" v-if="r.kind === 'error'">
      <span class="tag bad">{{ r.message }}</span>
      <span class="muted">耗时 {{ r.elapsed_ms ?? "—" }}ms</span>
    </div>
  </section>
</template>
