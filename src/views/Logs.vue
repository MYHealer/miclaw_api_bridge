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
  if (!("__TAURI_INTERNALS__" in window)) {
    rows.value = [
      {
        ts: Date.now(),
        kind: "request",
        path: "/osbot/pc/llm/v1/chat/completions",
        model: "mimo-pro",
        stream: true,
      },
      {
        ts: Date.now() - 42,
        kind: "response",
        path: "/osbot/pc/llm/v1/responses",
        status: 200,
        elapsed_ms: 1180,
      },
    ];
    return;
  }
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

function logLabel(r: LogRow) {
  if (r.kind === "request") return "request";
  if (r.kind === "response") return `status ${r.status ?? "—"}`;
  return "error";
}

function clear() {
  rows.value = [];
}
</script>

<template>
  <section class="panel logs-head">
    <div class="panel-heading">
      <p class="section-number">02</p>
      <div>
        <h2>实时事件</h2>
        <p>仅记录代理元数据，不记录 prompt、响应正文或 token 内容。</p>
      </div>
    </div>
    <div class="log-toolbar">
      <span class="state-line warn">{{ rows.length }} / {{ max }}</span>
      <button class="line-action" @click="clear">清空</button>
    </div>
  </section>

  <section class="panel empty-state" v-if="rows.length === 0">
    <span class="section-number">03</span>
    <h2>等待请求</h2>
    <p>启动代理后，用 OpenAI、Responses 或 Anthropic 客户端连接本地端口。</p>
  </section>

  <section v-else class="log-list" aria-label="代理日志列表">
    <article class="log-row" v-for="(r, i) in rows" :key="i">
      <span class="row-index">{{ String(i + 1).padStart(2, "0") }}</span>
      <span :class="['state-line', tagClass(r.kind, r.status)]">{{ logLabel(r) }}</span>
      <code>{{ r.path || "—" }}</code>
      <span class="muted">{{ fmtTime(r.ts) }}</span>
      <span v-if="r.kind === 'request'" class="muted">
        model={{ r.model || "—" }} · stream={{ r.stream ? "true" : "false" }}
      </span>
      <span v-else-if="r.kind === 'response'" class="muted">
        {{ r.elapsed_ms ?? "—" }}ms
      </span>
      <span v-else class="muted">{{ r.message }} · {{ r.elapsed_ms ?? "—" }}ms</span>
    </article>
  </section>
</template>
