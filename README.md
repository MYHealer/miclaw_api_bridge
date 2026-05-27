# mimo-bridge

把小米账号登录获得的 mimo 大模型，桥接成本地的 OpenAI / Anthropic 兼容接口，供 Cline、Claude Code、Cherry Studio 等客户端使用。

## 总体架构

```
┌─────────────┐  小米账号 OAuth   ┌───────────────────────┐  HTTP   ┌─────────────────┐
│ mimo-bridge │ ───────────────→ │ account.xiaomi.com    │         │ mimo-companion  │
│ (Tauri 桌面) │                  │ /pass/serviceLoginAuth2│         │ (Android APK)   │
│             │  cookie三件套     │                        │         │                 │
│             │ ←───────────────  └───────────────────────┘         │ /sign /fid      │
│             │                                                       │ /health         │
│             │                                                       └────────┬────────┘
│             │  POST /v1/chat/completions ──────────────────────────────────────┐
│             │  POST /v1/responses                                              │
│             │  POST /v1/messages (Anthropic) ─────► OpenAI Responses 转换     │
│             │  POST /v1/embeddings                                              │
└─────────────┘                                                                  │
                                                                                  ▼
                                                              api.miclaw.xiaomi.net
                                                              /osbot/api/llm/v1/...
```

- **小米账号**：完整复刻 miclaw 的 `MiPassportLoginActivity`（`sid=xiaomihome`、密码 MD5 大写、`_sign/qs/callback` 防重放、2FA 短信/邮箱）。
- **mimo 调用**：通过 cookie `cUserId+userId+serviceToken` 鉴权，附带 `x-device-fid/signature/ts` 设备签名头。
- **设备签名**：来自小米手机 MIUI 系统服务 `com.xiaomi.account.action.SECURITY_DEVICE_SIGN`（TEE 私钥），无法纯软复刻。本项目通过 `mimo-companion` APK 在用户手机上代理签名。
- **本地服务**：axum 监听 `127.0.0.1:8765`，暴露 OpenAI Chat / Responses / Embeddings 与 Anthropic Messages 端点，OpenAI 接口透传，Anthropic 接口做 Responses ↔ Messages 双向 SSE 翻译。

## 开发

```bash
pnpm install
pnpm tauri dev
```

## 路线图

- [x] M1 Tauri + Vue + Rust 脚手架，命令矩阵
- [ ] M2 OAuth 2FA 在 CLI 测试用例中跑通
- [ ] M3 mimo-companion APK + Rust 客户端
- [ ] M4 mimo 流式调用：CLI 模式真实命中 `/chat/completions` 与 `/responses`
- [ ] M5 Anthropic Messages 流式还原（已具备代码骨架，待联调）
- [ ] M6 端到端：Cline / Claude Code / Cherry Studio 接入
- [ ] M7 macOS dmg 打包、Windows MSI

## 兼容性

| 路径                   | 协议                       | 说明                              |
| ---------------------- | -------------------------- | --------------------------------- |
| `GET  /v1/models`      | OpenAI                     | 列出可用模型                      |
| `POST /v1/chat/completions` | OpenAI Chat（含 SSE） | 透传给 mimo                       |
| `POST /v1/responses`   | OpenAI Responses（含 SSE） | 透传给 mimo                       |
| `POST /v1/embeddings`  | OpenAI Embeddings          | 透传给 mimo（默认 siliconflow）   |
| `POST /v1/messages`    | Anthropic Messages（含 SSE）| 内部转 Responses，事件流双向翻译  |

## 安全

- 账号密码不会持久化，登录成功后只保留 cookie 三件套（默认写入 OS keychain）。
- 设备签名通过 LAN 或 ADB reverse 与 companion 通信，建议优先使用 ADB reverse。
- 本项目仅供学习交流，使用者自行承担风险。
