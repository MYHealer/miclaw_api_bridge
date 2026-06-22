<script setup lang="ts">
import { onMounted, ref } from "vue";
import { useRouter } from "vue-router";
import { api } from "../api";

const router = useRouter();
const ready = ref(false);
const configured = ref(false);
const password = ref("");
const confirm = ref("");
const busy = ref(false);
const error = ref("");

async function load() {
  try {
    const s = await api.adminSession();
    configured.value = s.configured;
    if (s.configured && s.authenticated) {
      router.replace("/dashboard");
      return;
    }
  } catch (e: any) {
    error.value = String(e);
  } finally {
    ready.value = true;
  }
}

async function submit() {
  error.value = "";
  if (!configured.value && password.value !== confirm.value) {
    error.value = "两次输入的密码不一致";
    return;
  }
  busy.value = true;
  try {
    if (configured.value) {
      await api.adminLogin(password.value);
    } else {
      await api.adminSetup(password.value);
    }
    router.replace("/dashboard");
  } catch (e: any) {
    error.value = e?.message ?? String(e);
  } finally {
    busy.value = false;
  }
}

onMounted(load);
</script>

<template>
  <section class="login-grid" v-if="ready">
    <div class="panel login-form">
      <div class="panel-heading">
        <p class="section-number">00</p>
        <div>
          <h2>{{ configured ? "后台登录" : "设置管理密码" }}</h2>
          <p v-if="configured">输入管理密码以进入后台。</p>
          <p v-else>首次访问，请为管理后台设置一个密码（至少 6 位）。设置后访问后台都需要登录。</p>
        </div>
      </div>

      <form class="form-stack" @submit.prevent="submit">
        <label>
          <span>管理密码</span>
          <input
            type="password"
            v-model="password"
            autocomplete="current-password"
            placeholder="••••••"
          />
        </label>
        <label v-if="!configured">
          <span>确认密码</span>
          <input type="password" v-model="confirm" autocomplete="new-password" />
        </label>
        <button
          class="primary-action"
          type="submit"
          :disabled="busy || !password || (!configured && !confirm)"
        >
          {{ configured ? "登录" : "设置并进入" }}
        </button>
      </form>

      <p v-if="error" class="notice bad">{{ error }}</p>
    </div>

    <aside class="panel auth-steps">
      <div class="panel-heading compact">
        <p class="section-number">!</p>
        <div>
          <h2>关于鉴权</h2>
          <p>这是 WebUI 后台的访问密码，与小米账号无关。</p>
        </div>
      </div>
      <ol>
        <li><span>01</span><strong>本地优先</strong><p>密码以 argon2 哈希保存在本机。</p></li>
        <li><span>02</span><strong>会话 Cookie</strong><p>登录后通过 HttpOnly Cookie 维持。</p></li>
        <li><span>03</span><strong>忘记密码</strong><p>删除数据目录下 security.json 可重置。</p></li>
      </ol>
    </aside>
  </section>
</template>
