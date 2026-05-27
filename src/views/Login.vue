<script setup lang="ts">
import { ref } from "vue";
import { api } from "../api";

const account = ref("");
const password = ref("");
const captcha = ref("");
const captchaUrl = ref<string | null>(null);
const flow = ref<"idle" | "captcha" | "two_factor" | "done" | "fail">("idle");
const options = ref<number[]>([]);
const flag = ref<number>(8);
const ticket = ref("");
const busy = ref(false);
const message = ref("");
const error = ref("");

async function doLogin() {
  busy.value = true;
  error.value = "";
  message.value = "";
  try {
    const res = await api.login(account.value, password.value, captcha.value || undefined);
    if (res.outcome === "authenticated") {
      flow.value = "done";
      message.value = `登录成功：${res.nick ?? account.value}`;
    } else if (res.outcome === "two_factor_required") {
      flow.value = "two_factor";
      options.value = res.options;
      flag.value = res.options.includes(8) ? 8 : res.options[0];
      message.value = "需要二步验证。";
    } else if (res.outcome === "captcha_required") {
      flow.value = "captcha";
      captchaUrl.value = res.captcha_url;
      message.value = "需要图形验证码，请输入后重试。";
    } else {
      flow.value = "fail";
      error.value = `登录失败 (${res.code})：${res.description}`;
    }
  } catch (e: any) {
    error.value = String(e);
    flow.value = "fail";
  } finally {
    busy.value = false;
  }
}

async function sendTicket() {
  busy.value = true;
  try {
    await api.sendTicket(flag.value);
    message.value = flag.value === 4 ? "已发送短信验证码。" : "已发送邮箱验证码。";
  } catch (e: any) {
    error.value = String(e);
  } finally {
    busy.value = false;
  }
}

async function verify() {
  busy.value = true;
  try {
    await api.verifyTicket(flag.value, ticket.value);
    message.value = "验证成功。";
    flow.value = "done";
  } catch (e: any) {
    error.value = String(e);
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <h1>小米账号登录</h1>
  <p class="muted">使用账号密码登录小米账号，获得 mimo 调用所需的 serviceToken。账号密码不会上传。</p>

  <section class="card">
    <div class="row">
      <div class="grow">
        <label>账号 / 邮箱 / 手机号</label>
        <input v-model="account" placeholder="user@example.com" />
      </div>
    </div>
    <div class="row">
      <div class="grow">
        <label>密码</label>
        <input type="password" v-model="password" />
      </div>
    </div>
    <div class="row" v-if="flow === 'captcha'">
      <div class="grow">
        <label>图形验证码</label>
        <img v-if="captchaUrl" :src="captchaUrl" alt="captcha" />
        <input v-model="captcha" />
      </div>
    </div>
    <div class="row">
      <button :disabled="busy" @click="doLogin">登录</button>
      <span v-if="message" class="tag ok">{{ message }}</span>
      <span v-if="error" class="tag bad">{{ error }}</span>
    </div>
  </section>

  <section class="card" v-if="flow === 'two_factor'">
    <h2>二步验证</h2>
    <div class="row">
      <div class="grow">
        <label>验证方式</label>
        <select v-model.number="flag">
          <option v-for="o in options" :key="o" :value="o">
            {{ o === 4 ? "短信" : o === 8 ? "邮箱" : `flag=${o}` }}
          </option>
        </select>
      </div>
      <button class="ghost" :disabled="busy" @click="sendTicket">发送验证码</button>
    </div>
    <div class="row">
      <div class="grow">
        <label>验证码</label>
        <input v-model="ticket" />
      </div>
      <button :disabled="busy || !ticket" @click="verify">完成验证</button>
    </div>
  </section>
</template>
