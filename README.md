# mimo-bridge

把小米账号登录得到的 mimo 大模型，桥接成本地的 OpenAI / Anthropic 兼容接口，供 Cline、Claude Code、Cherry Studio 等客户端使用。

## 当前实现

```
┌────────────────┐  小米账号 OAuth   ┌─────────────────────────┐
│ mimo-bridge    │ ────────────────→ │ account.xiaomi.com       │
│ (Tauri + Vue3) │                  │ /pass/serviceLogin{,Auth2} │
│                │ ←────────────────  └─────────────────────────┘
│                │  serviceToken+cUserId
│                │
│                │  POST /v1/chat/completions     ────────────► api.miclaw.xiaomi.net
│                │  POST /v1/messages (Anthropic)              /osbot/pc/llm/v1/chat/completions
│                │  GET  /v1/models                                 (OpenAI Chat + SSE)
└────────────────┘
```

- **小米账号**：复刻自反编译的 `MiPassportLoginActivity`：`sid=xiaomihome`、密码 MD5 大写、`_sign/qs/callback` 防重放、2FA 短信(flag=4)/邮箱(flag=8)。
- **mimo 调用**：用登录得到的 `serviceToken+cUserId` cookie 直连 `api.miclaw.xiaomi.net`（PC 端口）。**无需设备签名**。
- **本地服务**：axum 监听 `127.0.0.1:8765`，OpenAI Chat 透传，Anthropic Messages 流式翻译为 OpenAI Chat 再转回 Anthropic SSE 事件。

## 开发与运行

```bash
# 一次性装依赖
pnpm install

# 起桌面端
pnpm tauri dev

# 仅校验后端
cd src-tauri && cargo check
```

### 真账号 OAuth 集成测试

不会进入 CI，只在本地用环境变量手动触发：

```bash
cd src-tauri

# 先发 2FA 验证码
MIMO_BRIDGE_SMOKE_ACCOUNT=user@example.com \
MIMO_BRIDGE_SMOKE_PASSWORD='secret' \
cargo test --test smoke_login -- --ignored --nocapture
# 输出会指引你在邮箱/短信里收到验证码后再次执行：

MIMO_BRIDGE_SMOKE_ACCOUNT=user@example.com \
MIMO_BRIDGE_SMOKE_PASSWORD='secret' \
MIMO_BRIDGE_SMOKE_2FA_FLAG=8 \
MIMO_BRIDGE_SMOKE_2FA_TICKET='123456' \
MIMO_BRIDGE_SMOKE_CHAT=1 \
cargo test --test smoke_login -- --ignored --nocapture
```

## 客户端接入

### OpenAI 兼容（Cline / Cherry Studio / OpenAI SDK）

```
Base URL: http://127.0.0.1:8765/v1
API Key:   anything
Model:     mimo-omni
```

### Anthropic 兼容（Claude Code / 走 ANTHROPIC_BASE_URL 的客户端）

```
Base URL: http://127.0.0.1:8765
API Key:   anything
Model:     mimo-omni  (或前缀 anthropic/，会自动剥离)
```

### curl 流式探测

```bash
curl http://127.0.0.1:8765/v1/chat/completions \
  -H 'content-type: application/json' \
  -d '{"model":"mimo-omni","stream":true,"messages":[{"role":"user","content":"hi"}]}'
```

## 路线图

- [x] M1 Tauri + Vue + Rust 脚手架，命令矩阵
- [x] M2 OAuth 2FA：smoke 测试在本地用真账号跑通（待你执行）
- [x] M3 mimo PC 客户端（无需 sdc 设备签名 / Companion APK）
- [x] M4 axum 本地代理：OpenAI Chat 透传 + Anthropic Messages 流式翻译
- [ ] M5 Vue 前端真实联调（登录页、状态、日志）
- [ ] M6 端到端：Cline / Claude Code / Cherry Studio 接入验证
- [ ] M7 macOS dmg 打包、Windows MSI

## 安全提示

- 账号密码不会持久化，只把 `serviceToken/passToken/cUserId/userId/ssecurity/nick` 几项写入应用数据目录的 `session.json`（明文）。后续会迁移到 keyring。
- 本项目仅供学习交流，使用者自行承担风险。
