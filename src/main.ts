import { createApp } from "vue";
import { createPinia } from "pinia";
import { createRouter, createWebHashHistory } from "vue-router";
import App from "./App.vue";
import Dashboard from "./views/Dashboard.vue";
import Login from "./views/Login.vue";
import Logs from "./views/Logs.vue";
import "./styles.css";

const router = createRouter({
  history: createWebHashHistory(),
  routes: [
    { path: "/", redirect: "/dashboard" },
    { path: "/dashboard", component: Dashboard },
    { path: "/login", component: Login },
    { path: "/logs", component: Logs },
  ],
});

createApp(App).use(createPinia()).use(router).mount("#app");
