import { createApp } from "vue";
import { createPinia } from "pinia";
import { createRouter, createWebHashHistory } from "vue-router";
import App from "./App.vue";
import Dashboard from "./views/Dashboard.vue";
import Login from "./views/Login.vue";
import Logs from "./views/Logs.vue";
import AdminLogin from "./views/AdminLogin.vue";
import ApiKeys from "./views/ApiKeys.vue";
import Usage from "./views/Usage.vue";
import { api } from "./api";
import "./styles.css";

const router = createRouter({
  history: createWebHashHistory(),
  routes: [
    { path: "/", redirect: "/dashboard" },
    { path: "/dashboard", component: Dashboard },
    { path: "/login", component: Login },
    { path: "/logs", component: Logs },
    { path: "/keys", component: ApiKeys },
    { path: "/usage", component: Usage },
    { path: "/admin-login", component: AdminLogin },
  ],
});

// Admin-session guard: once a password is configured, redirect to the login
// page until the session cookie is valid. The login page itself is exempt.
router.beforeEach(async (to) => {
  if (to.path === "/admin-login") return true;
  try {
    const s = await api.adminSession();
    if (s.configured && !s.authenticated) return "/admin-login";
  } catch {
    /* if the status check fails, let the page load and surface errors itself */
  }
  return true;
});

createApp(App).use(createPinia()).use(router).mount("#app");
