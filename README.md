# mimo-bridge

把小米账号登录得到的 mimo 大模型，桥接成本地的 OpenAI / Anthropic 兼容接口，供 Cline、Claude Code、Cherry Studio 等客户端使用。

## 当前实现

```
┌────────────────┐ 1. account.xiaomi.com /pass/serviceLoginAuth2 (sid=miclaw)
│ mimo-bridge    │ ─────────────────────────────────────────────► passToken + cUserId
│ (Tauri + Vue3) │
│                │ 2. /pass/serviceLogin?sid=osbotapi (UA=miNative PC, 7 cookies)
│                │ ─────────────────────────────────────────────► loc + nonce + ssecurity
│                │
│                │ 3. <loc>&clientSign=base64(sha1("nonce=N&ssecurity")) (no cookies)
│                │ ─────────────────────────────────────────────► serviceToken
│                │
│                │ 4. POST api.miclaw.xiaomi.net/osbot/pc/llm/v1/chat/completions
│                │       Cookie: serviceToken+cUserId, UA=node
│                │ ─────────────────────────────────────────────► OpenAI SSE chunks
│                │
│ axum :8765     │ 暴露:
│  /v1/chat/completions   ← OpenAI Chat 透传
│  /v1/messages           ← Anthropic Messages，SSE 双向翻译
│  /v1/models             ← 模型列表
└────────────────┘
```

### 关键事实（从源码与 HAR 实测对齐得来）

- 第一阶段密码登录 `sid=miclaw`，密码 MD5 大写哈希；2FA 短信 `flag=4` / 邮箱 `flag=8`，发码 / 验证均挂在同一 cookie jar。
- 第二阶段换 mimo 专用 `serviceToken`：`sid=osbotapi`，UA 必须是 macOS miclaw 的 `miNative PC/...`，cookie 7 件套：`passToken / userId / cUserId / deviceId / uDevId / uLocale / pass_ua`。
- `deviceId = "pc_" + md5_hex(IOPlatformUUID.toLowerCase())`，与反编译的 `dist-electron/libs/xiaomi/deviceid.js` 完全一致。
- `uDevId = base64(sha1(userId + deviceId))`。
- 拿 serviceToken 走两步：Phase 1 GET 带 7 cookies → 拿 `loc + nonce + ssecurity`；Phase 2 GET `<loc>&clientSign=<sig>` **不带 cookie**，从 Set-Cookie 解出 `serviceToken`。`sig = url_encode(base64(sha1("nonce=N&ssecurity")))`。
- nonce 是大整数，必须从 raw JSON 抽取（serde_json / JSON.parse 会丢精度）。
- mimo 真实调用只需 `Cookie: serviceToken=...; cUserId=...`，UA `node`，无设备签名。
- 401 时自动用 passToken 走 osbotapi 双阶段刷新一次。
- 凭证写 OS keyring（macOS Keychain / Windows DPAPI / Linux SecretService），磁盘上不留明文。

## 开发与运行

```bash
pnpm install
pnpm tauri dev          # 起桌面端
cd src-tauri && cargo check    # 仅校验后端
cargo test --lib                # 跑单元测试（Anthropic 翻译器等）
```

### 真账号 OAuth 集成测试

只在本地用环境变量手动触发：

```bash
cd src-tauri

# 先发 2FA 验证码
MIMO_BRIDGE_SMOKE_ACCOUNT=user@example.com \
MIMO_BRIDGE_SMOKE_PASSWORD='secret' \
cargo test --test smoke_login -- --ignored --nocapture

# 收到短信/邮箱验证码后:
MIMO_BRIDGE_SMOKE_ACCOUNT=user@example.com \
MIMO_BRIDGE_SMOKE_PASSWORD='secret' \
MIMO_BRIDGE_SMOKE_2FA_FLAG=4 \
MIMO_BRIDGE_SMOKE_2FA_TICKET='123456' \
MIMO_BRIDGE_SMOKE_CHAT=1 \
cargo test --test smoke_login -- --ignored --nocapture
```

## 打包

```bash
pnpm tauri build
# 产物在 src-tauri/target/release/bundle/dmg/*.dmg 和 .../macos/*.app
```

## 客户端接入

### OpenAI 兼容（Cline / Cherry Studio / OpenAI SDK）

```
Base URL: http://127.0.0.1:8765/v1
API Key:  anything
Model:    mimo-omni
```

### Anthropic 兼容（Claude Code / 走 ANTHROPIC_BASE_URL 的客户端）

```
Base URL: http://127.0.0.1:8765
API Key:  anything
Model:    mimo-omni    (或前缀 anthropic/ 会自动剥离)
```

### curl 流式探测

```bash
# OpenAI Chat
curl -N http://127.0.0.1:8765/v1/chat/completions \
  -H 'content-type: application/json' \
  -d '{"model":"mimo-omni","stream":true,"messages":[{"role":"user","content":"你好"}]}'

# Anthropic Messages
curl -N http://127.0.0.1:8765/v1/messages \
  -H 'content-type: application/json' \
  -H 'anthropic-version: 2023-06-01' \
  -d '{"model":"mimo-omni","max_tokens":256,"stream":true,"messages":[{"role":"user","content":"你好"}]}'
```

## 路线图

- [x] M1 Tauri + Vue + Rust 脚手架
- [x] M2 OAuth 2FA（smoke 真账号通过）
- [x] M3 mimo PC 客户端（osbotapi 双阶段换 token，自动刷新）
- [x] M4 axum 本地代理：OpenAI Chat 透传 + Anthropic Messages 流式翻译（含单测）
- [x] M5 Vue 前端：登录、概览、实时日志面板
- [x] M6 凭证写 keyring，session.json 不留明文
- [x] M7 macOS dmg 打包

## 安全提示

- 账号密码绝不持久化；登录成功后只把 `passToken / serviceToken / cUserId / userId / ssecurity / nick` 写到 OS keyring。
- 本项目仅供学习交流，使用者自行承担风险。
