<script setup lang="ts">
import { RouterView, RouterLink } from "vue-router";
</script>

<template>
  <div class="app-shell">
    <aside class="sidebar">
      <div class="brand">mimo-bridge</div>
      <nav>
        <RouterLink to="/dashboard">概览</RouterLink>
        <RouterLink to="/login">小米账号</RouterLink>
        <RouterLink to="/logs">日志</RouterLink>
      </nav>
      <footer>v0.1.0</footer>
    </aside>
    <main class="content">
      <RouterView />
    </main>
  </div>
</template>
