<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from "vue";
import { api } from "../api";

interface LogRow {
  ts: number;
  kind: "request" | "response" | "error" | string;
  path?: string;
  model?: string;
  stream?: boolean;
  status?: number;
  elapsed_ms?: number;
  message?: string;
  body?: unknown;
}

const rows = ref<LogRow[]>([]);
const max = 500;
const verbose = ref(false);
const expanded = ref<Set<LogRow>>(new Set());
let events: EventSource | null = null;

function push(row: LogRow) {
  rows.value.unshift(row);
  if (rows.value.length > max) rows.value.length = max;
}

onMounted(async () => {
  try {
    verbose.value = (await api.getVerboseLogs()).enabled;
  } catch {
    /* keep default */
  }
  const resp = await fetch("/api/logs");
  if (resp.ok) rows.value = await resp.json();
  events = new EventSource("/api/logs/stream");
  events.onmessage = (event) => push(JSON.parse(event.data) as LogRow);
});

onBeforeUnmount(() => {
  events?.close();
});

async function toggleVerbose() {
  const next = !verbose.value;
  try {
    verbose.value = (await api.setVerboseLogs(next)).enabled;
  } catch {
    /* leave unchanged on failure */
  }
}

function canExpand(r: LogRow) {
  return r.body != null && (r.kind === "request" || r.kind === "response");
}

function toggleRow(r: LogRow) {
  if (!canExpand(r)) return;
  const next = new Set(expanded.value);
  next.has(r) ? next.delete(r) : next.add(r);
  expanded.value = next;
}

function pretty(body: unknown) {
  if (typeof body === "string") return body;
  try {
    return JSON.stringify(body, null, 2);
  } catch {
    return String(body);
  }
}

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
  expanded.value = new Set();
}
</script>

<template>
  <section class="panel logs-head">
    <div class="panel-heading">
      <p class="section-number">02</p>
      <div>
        <h2>实时事件</h2>
        <p>默认仅记录代理元数据；开启“详细日志”后会额外记录请求正文（含 prompt），点击请求行即可展开。</p>
      </div>
    </div>
    <div class="log-toolbar">
      <span class="state-line warn">{{ rows.length }} / {{ max }}</span>
      <button class="line-action" :class="{ active: verbose }" @click="toggleVerbose">
        详细日志 {{ verbose ? "开" : "关" }}
      </button>
      <button class="line-action" @click="clear">清空</button>
    </div>
  </section>

  <section class="panel empty-state" v-if="rows.length === 0">
    <span class="section-number">03</span>
    <h2>等待请求</h2>
    <p>启动代理后，用 OpenAI、Responses 或 Anthropic 客户端连接本地端口。</p>
  </section>

  <section v-else class="log-list" aria-label="代理日志列表">
    <template v-for="(r, i) in rows" :key="i">
      <article
        class="log-row"
        :class="{ clickable: canExpand(r) }"
        @click="toggleRow(r)"
      >
        <span class="row-index">{{ String(i + 1).padStart(2, "0") }}</span>
        <span :class="['state-line', tagClass(r.kind, r.status)]">{{ logLabel(r) }}</span>
        <code>{{ r.path || "—" }}</code>
        <span class="muted">{{ fmtTime(r.ts) }}</span>
        <span v-if="r.kind === 'request'" class="muted">
          model={{ r.model || "—" }} · stream={{ r.stream ? "true" : "false" }}
          <span v-if="canExpand(r)">· {{ expanded.has(r) ? "收起" : "展开" }}</span>
        </span>
        <span v-else-if="r.kind === 'response'" class="muted">
          {{ r.elapsed_ms ?? "—" }}ms
          <span v-if="canExpand(r)">· {{ expanded.has(r) ? "收起" : "展开" }}</span>
        </span>
        <span v-else class="muted">{{ r.message }} · {{ r.elapsed_ms ?? "—" }}ms</span>
      </article>
      <pre v-if="expanded.has(r) && r.body != null" class="log-detail">{{ pretty(r.body) }}</pre>
    </template>
  </section>
</template>

<style scoped>
.line-action.active {
  background: var(--ink);
  color: var(--bg);
}

.log-row.clickable {
  cursor: pointer;
}

.log-row.clickable:hover {
  background: var(--surface-soft);
}

.log-detail {
  margin: 0 0 0 120px;
  padding: 18px 32px 18px 48px;
  border-bottom: 1px solid var(--line);
  background: var(--surface-soft);
  border-radius: 0;
  color: var(--ink);
  font-family: "SF Mono", "JetBrains Mono", ui-monospace, Menlo, Consolas, monospace;
  font-size: 12px;
  line-height: 1.6;
  white-space: pre-wrap;
  word-break: break-word;
  overflow-x: auto;
}
</style>
