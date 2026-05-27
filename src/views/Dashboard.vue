<script setup lang="ts">
import { onMounted, ref } from "vue";
import { api, AuthSnapshot, ModelInfo, ProxySnapshot } from "../api";

const auth = ref<AuthSnapshot | null>(null);
const proxy = ref<ProxySnapshot | null>(null);
const models = ref<ModelInfo[]>([]);
const portInput = ref<number>(8765);
const busy = ref(false);
const err = ref("");

async function refreshAll() {
  err.value = "";
  try {
    auth.value = await api.authStatus();
    proxy.value = await api.proxyStatus();
    models.value = await api.listModels();
    portInput.value = proxy.value.port;
  } catch (e: any) {
    err.value = String(e);
  }
}

async function toggleProxy() {
  busy.value = true;
  try {
    proxy.value = proxy.value?.running ? await api.stopProxy() : await api.startProxy();
  } catch (e: any) {
    err.value = String(e);
  } finally {
    busy.value = false;
  }
}

async function applyPort() {
  busy.value = true;
  try {
    proxy.value = await api.setProxyPort(portInput.value);
  } catch (e: any) {
    err.value = String(e);
  } finally {
    busy.value = false;
  }
}

async function refreshAuth() {
  busy.value = true;
  try {
    auth.value = await api.refreshSession();
  } catch (e: any) {
    err.value = String(e);
  } finally {
    busy.value = false;
  }
}

async function logout() {
  await api.logout();
  await refreshAll();
}

onMounted(refreshAll);
</script>

<template>
  <h1>概览</h1>
  <p v-if="err" class="tag bad">{{ err }}</p>

  <section class="card">
    <h2>本地代理</h2>
    <div class="row">
      <span class="tag" :class="proxy?.running ? 'ok' : 'warn'">
        {{ proxy?.running ? `运行中 ${proxy.addr ?? ''}` : "已停止" }}
      </span>
      <div class="grow"></div>
      <button :disabled="busy" @click="toggleProxy">
        {{ proxy?.running ? "停止" : "启动" }}
      </button>
    </div>
    <div class="row">
      <div class="grow">
        <label>监听端口</label>
        <input type="number" v-model.number="portInput" min="1024" max="65535" />
      </div>
      <button class="ghost" :disabled="busy" @click="applyPort">应用</button>
    </div>
    <p class="muted">
      OpenAI 客户端 baseURL：<code>http://127.0.0.1:{{ proxy?.port ?? 8765 }}/v1</code><br />
      Anthropic 客户端 baseURL：<code>http://127.0.0.1:{{ proxy?.port ?? 8765 }}</code><br />
      <span class="muted">无需 API Key（任意字符串即可）。</span>
    </p>
  </section>

  <section class="card">
    <h2>账号</h2>
    <div class="row">
      <span class="tag" :class="auth?.authenticated ? 'ok' : 'bad'">
        {{ auth?.authenticated ? `已登录 ${auth.nick ?? auth.user_id ?? ''}` : "未登录" }}
      </span>
      <div class="grow"></div>
      <button class="ghost" :disabled="busy || !auth?.authenticated" @click="refreshAuth">
        刷新令牌
      </button>
      <button class="danger" :disabled="busy || !auth?.authenticated" @click="logout">退出</button>
    </div>
  </section>

  <section class="card">
    <h2>可用模型</h2>
    <ul>
      <li v-for="m in models" :key="m.id">
        <code>{{ m.id }}</code> <span class="muted">— {{ m.family }}</span>
      </li>
    </ul>
  </section>
</template>
