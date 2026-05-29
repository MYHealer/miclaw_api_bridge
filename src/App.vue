<script setup lang="ts">
import { computed, onMounted, ref, watchEffect } from "vue";
import { RouterLink, RouterView, useRoute } from "vue-router";
import appIcon from "../src-tauri/icons/icon.png";

type Theme = "light" | "dark";

const route = useRoute();
const theme = ref<Theme>("light");
const patternLetters = ["M", "I", "M", "O"];
const patternRows = Array.from({ length: 8 }, (_, row) =>
  Array.from({ length: 18 }, (_, col) => patternLetters[(col + (row % 2)) % patternLetters.length]),
);

const pageTitle = computed(() => {
  if (route.path.includes("login")) return "小米账号";
  if (route.path.includes("logs")) return "代理日志";
  return "本地代理";
});

function applyTheme(next: Theme) {
  theme.value = next;
  localStorage.setItem("miclaw-theme", next);
}

function toggleTheme() {
  applyTheme(theme.value === "dark" ? "light" : "dark");
}

onMounted(() => {
  const saved = localStorage.getItem("miclaw-theme") as Theme | null;
  if (saved === "light" || saved === "dark") {
    theme.value = saved;
    return;
  }
  theme.value = window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
});

watchEffect(() => {
  document.documentElement.dataset.theme = theme.value;
});
</script>

<template>
  <div class="app-shell">
    <header class="topbar">
      <RouterLink class="brand-lockup" to="/dashboard" aria-label="miclaw_api_bridge dashboard">
        <img :src="appIcon" alt="" />
        <span>miclaw_api_bridge</span>
      </RouterLink>

      <nav class="topnav" aria-label="主导航">
        <RouterLink to="/dashboard">Proxy</RouterLink>
        <RouterLink to="/login">Account</RouterLink>
        <RouterLink to="/logs">Logs</RouterLink>
      </nav>

      <button
        class="icon-button"
        type="button"
        :title="theme === 'dark' ? '切换浅色模式' : '切换深色模式'"
        :aria-label="theme === 'dark' ? '切换浅色模式' : '切换深色模式'"
        @click="toggleTheme"
      >
        <svg v-if="theme === 'dark'" viewBox="0 0 24 24" aria-hidden="true">
          <path d="M12 3v2.1M12 18.9V21M4.4 4.4l1.5 1.5M18.1 18.1l1.5 1.5M3 12h2.1M18.9 12H21M4.4 19.6l1.5-1.5M18.1 5.9l1.5-1.5" />
          <circle cx="12" cy="12" r="4.2" />
        </svg>
        <svg v-else viewBox="0 0 24 24" aria-hidden="true">
          <path d="M20.4 14.2A7.7 7.7 0 0 1 9.8 3.6 8.7 8.7 0 1 0 20.4 14.2Z" />
        </svg>
      </button>
    </header>

    <section class="page-hero" aria-label="当前页面">
      <div class="pattern" aria-hidden="true">
        <span v-for="(row, rowIndex) in patternRows" :key="rowIndex" class="pattern-row">
          <b v-for="(letter, colIndex) in row" :key="`${rowIndex}-${colIndex}`">{{ letter }}</b>
        </span>
      </div>
      <p class="section-number">01</p>
      <h1>{{ pageTitle }}</h1>
      <p class="hero-copy">将 Xiaomi miclaw 模型转接成本地 OpenAI / Anthropic 兼容端点。</p>
    </section>

    <main class="content">
      <RouterView />
    </main>
  </div>
</template>
