<script setup lang="ts">
import { onMounted, ref } from "vue";
import { api, ApiKeyView } from "../api";

const keys = ref<ApiKeyView[]>([]);
const required = ref(false);
const newName = ref("");
const createdSecret = ref<string | null>(null);
const busy = ref(false);
const error = ref("");
const copied = ref(false);

async function load() {
  error.value = "";
  try {
    keys.value = await api.listKeys();
    required.value = (await api.getApiKeyRequired()).required;
  } catch (e: any) {
    error.value = e?.message ?? String(e);
  }
}

async function create() {
  busy.value = true;
  error.value = "";
  createdSecret.value = null;
  try {
    const res = await api.createKey(newName.value || "未命名");
    createdSecret.value = res.secret;
    newName.value = "";
    await load();
  } catch (e: any) {
    error.value = e?.message ?? String(e);
  } finally {
    busy.value = false;
  }
}

async function remove(id: string) {
  busy.value = true;
  try {
    await api.deleteKey(id);
    await load();
  } catch (e: any) {
    error.value = e?.message ?? String(e);
  } finally {
    busy.value = false;
  }
}

async function toggleRequired() {
  busy.value = true;
  try {
    const res = await api.setApiKeyRequired(!required.value);
    required.value = res.required;
  } catch (e: any) {
    error.value = e?.message ?? String(e);
  } finally {
    busy.value = false;
  }
}

async function copySecret() {
  if (!createdSecret.value) return;
  try {
    await navigator.clipboard.writeText(createdSecret.value);
    copied.value = true;
    setTimeout(() => (copied.value = false), 1500);
  } catch {
    /* clipboard may be unavailable over plain http */
  }
}

function fmt(ts: number | null) {
  return ts ? new Date(ts).toLocaleString() : "—";
}

onMounted(load);
</script>

<template>
  <p v-if="error" class="notice bad">{{ error }}</p>

  <section class="panel">
    <div class="panel-heading">
      <p class="section-number">01</p>
      <div>
        <h2>API Key</h2>
        <p>用于 /v1 接口的 Bearer 鉴权。开启“强制校验”后，没有有效 Key 的请求会被拒绝。</p>
      </div>
    </div>

    <div class="account-row">
      <span :class="['state-line', required ? 'ok' : 'warn']">
        强制校验：{{ required ? "已开启" : "已关闭" }}
      </span>
      <button class="line-action" :disabled="busy" @click="toggleRequired">
        {{ required ? "关闭强制" : "开启强制" }}
      </button>
    </div>
    <p v-if="!required" class="notice warn">
      当前未开启强制校验，任意（或不带）Key 都能访问 /v1，保持向后兼容。
    </p>
  </section>

  <section class="panel">
    <div class="panel-heading compact">
      <p class="section-number">02</p>
      <div>
        <h2>新建 Key</h2>
        <p>密钥明文仅在创建时显示一次，请立即保存。</p>
      </div>
    </div>
    <div class="two-factor-row">
      <label>
        <span>备注名</span>
        <input v-model="newName" placeholder="例如：我的笔记本" />
      </label>
      <button class="primary-action" :disabled="busy" @click="create">创建</button>
    </div>

    <div v-if="createdSecret" class="secret-reveal">
      <span class="label">新密钥（仅此一次可见）</span>
      <div class="secret-row">
        <code>{{ createdSecret }}</code>
        <button class="line-action" @click="copySecret">{{ copied ? "已复制" : "复制" }}</button>
      </div>
    </div>
  </section>

  <section class="panel">
    <div class="panel-heading compact">
      <p class="section-number">03</p>
      <div>
        <h2>已有 Key</h2>
        <p>{{ keys.length }} 个</p>
      </div>
    </div>
    <p v-if="!keys.length" class="notice">还没有创建任何 Key。</p>
    <div v-else class="model-table">
      <div v-for="k in keys" :key="k.id" class="key-row">
        <code>{{ k.prefix }}</code>
        <span>{{ k.name || "未命名" }}</span>
        <span class="muted">建于 {{ fmt(k.created_at) }}</span>
        <span class="muted">最近用 {{ fmt(k.last_used) }}</span>
        <button class="line-action danger" :disabled="busy" @click="remove(k.id)">删除</button>
      </div>
    </div>
  </section>
</template>

<style scoped>
.secret-reveal {
  margin-top: 1rem;
}
.secret-row {
  display: flex;
  align-items: center;
  gap: 0.75rem;
  margin-top: 0.35rem;
}
.secret-row code {
  flex: 1;
  word-break: break-all;
}
.key-row {
  display: grid;
  grid-template-columns: minmax(120px, 1fr) 1fr 1fr 1fr auto;
  align-items: center;
  gap: 0.75rem;
  padding: 0.6rem 0;
  border-bottom: 1px solid var(--hairline, rgba(128, 128, 128, 0.2));
}
.key-row .muted {
  opacity: 0.65;
  font-size: 0.85em;
}
@media (max-width: 720px) {
  .key-row {
    grid-template-columns: 1fr 1fr;
  }
}
</style>
