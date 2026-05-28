<div align="center">

# miclaw_api_bridge

**Run Xiaomi mimo locally as an OpenAI / Anthropic-compatible endpoint.**

Sign in with a miclaw-permissioned Xiaomi account, hit `http://127.0.0.1:8765` from any OpenAI / Claude client.

[![Built with Tauri](https://img.shields.io/badge/Tauri-2-blue?logo=tauri)](https://tauri.app/)
[![Rust](https://img.shields.io/badge/Rust-1.77+-orange?logo=rust)](https://www.rust-lang.org/)
[![Vue 3](https://img.shields.io/badge/Vue-3-42b883?logo=vue.js)](https://vuejs.org/)
[![Platform](https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-lightgrey)]()
[![License](https://img.shields.io/badge/license-MIT-green)](#license)

</div>

---

## What it does

miclaw_api_bridge logs into your Xiaomi account the same way the official **Xiaomi miclaw** desktop client does, then exposes the resulting `mimo` LLM through familiar local HTTP endpoints:

- `POST /v1/chat/completions` — OpenAI Chat Completions (drop-in for Cline, Cherry Studio, OpenAI SDKs, …)
- `POST /v1/responses` — OpenAI Responses API (native passthrough when available, Chat Completions compatibility fallback otherwise)
- `POST /v1/messages` — Anthropic Messages, with full SSE event translation (drop-in for Claude Code and any client honoring `ANTHROPIC_BASE_URL`)
- `GET /v1/models` — the eight verified model ids

> ⚠️ **Account requirement**: your Xiaomi account must already be approved for miclaw access. If `pnpm tauri dev` shows "需要 miclaw 内测权限" or the proxy returns 401 right after login, the account isn't allowlisted — apply through the official miclaw channel first.

Eight models are exposed, all routed through the official Xiaomi PC channel:

| Model id | Notes |
|---|---|
| `mimo-omni` | Multimodal, 256 K context (default) |
| `mimo-pro` | Reasoning model with `thinking` traces |
| `mimo-pro-1m` | Same as `mimo-pro` with 1 M context |
| `xiaomi/mimo-pro` | Alias for clients that prefer the Android-style id |
| `xiaomi/mimo-claw-0301` | Claw 0301 snapshot |
| `xiaomi/mimo-v2-omni` | v2 multimodal |
| `xiaomi/mimo-v2-pro` | v2 reasoning |
| `xiaomi/qwen35_9B` | Qwen 3.5 9B (vLLM-hosted, OpenAI-compatible) |

## Features

- 🔐 **Real Xiaomi OAuth** — username + password + SMS / email 2FA, exactly like the desktop client
- 🔄 **Auto token refresh** — `serviceToken` rotated transparently on 401
- 🔑 **Keychain-backed storage** — credentials live in macOS Keychain / Windows DPAPI / Linux Secret Service, never on disk in plaintext
- 🔌 **Two protocols, one bridge** — speaks both OpenAI Chat Completions and Anthropic Messages
- 📡 **Live request log** — built-in panel streams every proxy hit in real time
- 🖥 **Native desktop app** — Tauri + Vue 3, ~6 MB packaged

## Quick start

### From a release

1. Grab the `.dmg` (Apple Silicon) from the [Releases](../../releases) page
2. Drag **miclaw_api_bridge.app** to `/Applications`
3. Launch, sign in with your miclaw-permissioned Xiaomi account
4. Open the **Dashboard** tab and click **Start** — proxy is now listening on `127.0.0.1:8765`

> First launch on macOS: right-click → Open to bypass Gatekeeper (the app isn't notarized).

### Hooking up a client

**OpenAI-compatible (Cline, Cherry Studio, OpenAI SDK, …)**

```
Base URL: http://127.0.0.1:8765/v1
API key:  anything
Model:    mimo-pro      # or any model from /v1/models
```

**Anthropic-compatible (Claude Code, etc.)**

```
ANTHROPIC_BASE_URL=http://127.0.0.1:8765
ANTHROPIC_API_KEY=anything
Model: mimo-pro
```

**curl smoke test**

```bash
# OpenAI
curl -N http://127.0.0.1:8765/v1/chat/completions \
  -H 'content-type: application/json' \
  -d '{"model":"mimo-pro","stream":true,"messages":[{"role":"user","content":"hi"}]}'

# OpenAI Responses
curl -N http://127.0.0.1:8765/v1/responses \
  -H 'content-type: application/json' \
  -d '{"model":"mimo-pro","stream":true,"input":"hi"}'

# Anthropic
curl -N http://127.0.0.1:8765/v1/messages \
  -H 'content-type: application/json' \
  -H 'anthropic-version: 2023-06-01' \
  -d '{"model":"mimo-pro","max_tokens":256,"stream":true,"messages":[{"role":"user","content":"hi"}]}'
```

## Build from source

Prerequisites: Rust 1.77+, Node.js 20+, pnpm 9+.

```bash
git clone <this-repo> miclaw_api_bridge && cd miclaw_api_bridge
pnpm install
pnpm tauri dev          # run in dev mode
pnpm tauri build        # produce a .dmg / .msi / .AppImage
```

The signed bundle ends up in `src-tauri/target/release/bundle/`.

### Tests

```bash
cd src-tauri
cargo test --lib                           # unit tests (incl. Anthropic SSE translator)

# Real-account end-to-end smoke test (manual; never runs in CI)
MIMO_BRIDGE_SMOKE_ACCOUNT=user@example.com \
MIMO_BRIDGE_SMOKE_PASSWORD='secret' \
cargo test --test smoke_login -- --ignored --nocapture
# Then resolve the SMS / email challenge it prints and re-run with:
#   MIMO_BRIDGE_SMOKE_2FA_FLAG=4 MIMO_BRIDGE_SMOKE_2FA_TICKET=123456 MIMO_BRIDGE_SMOKE_CHAT=1
```

## Architecture

```
┌─────────────────┐
│ Vue 3 frontend  │   Tauri IPC    ┌──────────────────────────┐
│ Login / Status  │◄──────────────►│ Rust backend (Tauri host)│
│ Logs panel      │                └────────────┬─────────────┘
└─────────────────┘                             │
                                                │ axum on 127.0.0.1:8765
                                                ▼
                       ┌────────────────────────────────────────────┐
                       │ /v1/chat/completions  OpenAI passthrough   │
                       │ /v1/responses         OpenAI compat layer  │
                       │ /v1/messages          Anthropic ⇆ OpenAI   │
                       │ /v1/models            Static manifest      │
                       └────────────────────────┬───────────────────┘
                                                │
                                                ▼
                          api.miclaw.xiaomi.net /osbot/pc/llm/v1/...
                          (Cookie: serviceToken+cUserId, UA: node)
```

### Authentication flow

```
1. POST account.xiaomi.com/pass/serviceLoginAuth2 (sid=miclaw, MD5-upper of password)
   → 2FA challenge if required → identity/list, sendTicket, verifyTicket
   ⇒ passToken + cUserId + ssecurity

2. GET  account.xiaomi.com/pass/serviceLogin?sid=osbotapi
        UA = "miNative PC/...", Cookie = passToken+userId+cUserId+deviceId+uDevId+uLocale+pass_ua
   ⇒ loc + nonce + ssecurity   (nonce extracted from raw JSON to avoid f64 precision loss)

3. GET  <loc>&clientSign=<sig>          (no cookies)
        sig = url_encode( base64( sha1("nonce=N&ssecurity") ) )
   ⇒ Set-Cookie: serviceToken=...   (the token mimo accepts)

4. POST api.miclaw.xiaomi.net/osbot/pc/llm/v1/chat/completions
        Cookie: serviceToken+cUserId, UA: node
   ⇒ OpenAI-compatible Chat Completions (SSE when stream=true)
```

`deviceId` and `uDevId` are derived locally:

```
deviceId = "pc_" + md5_hex( IOPlatformUUID.toLowerCase() )           // matches macOS miclaw verbatim
uDevId   = base64( sha1( userId + deviceId ) )
```

A 401 from any mimo call triggers a transparent re-run of steps 2–3.

## Configuration

| Setting | Where | Default |
|---|---|---|
| Listen port | Dashboard ▸ "Listen port" | `8765` |
| OAuth `sid` | env `MIMO_BRIDGE_SID` | `miclaw` |
| Session storage | OS keyring (auto) | n/a |

## FAQ

**Is this a fork of miclaw?**
No — miclaw_api_bridge is an independent client that speaks the same protocol. No code is copied from the official client.

**Does it work without the official miclaw app installed?**
Yes. miclaw_api_bridge talks directly to Xiaomi's account and inference endpoints; the desktop client doesn't need to be installed. **Your account does, however, need miclaw access** — without it the inference API returns 401 even with a valid serviceToken.

**Why does the OAuth flow run twice?**
Xiaomi mints `serviceToken`s scoped to a specific `sid`. Password login uses `sid=miclaw`, but mimo only accepts tokens minted under `sid=osbotapi`. The second leg swaps the former for the latter using the long-lived `passToken`.

**Where are my credentials stored?**
Encrypted in your OS keyring (macOS Keychain / Windows DPAPI / Linux Secret Service). Only the session blob — `passToken / serviceToken / userId / cUserId / ssecurity / nick` — is kept; your password is never persisted.

**Can I run multiple accounts?**
Not yet. Tracking under [#multi-account](../../issues).

## Roadmap

- [x] Tauri + Vue + Rust scaffolding
- [x] Xiaomi OAuth with 2FA
- [x] mimo PC client with auto token refresh
- [x] OpenAI Chat Completions passthrough
- [x] Anthropic Messages bridge with SSE translation
- [x] Live log panel
- [x] Keychain-backed credential storage
- [x] macOS dmg packaging
- [ ] Universal macOS binary (Intel + Apple Silicon)
- [ ] Code signing & notarization
- [ ] Windows / Linux release pipeline
- [ ] Multi-account support
- [ ] Optional rate-limit / quota dashboard

## Contributing

Issues and PRs are welcome. Useful starting points:

- **Bug reports** — please include the `RUST_LOG=miclaw_api_bridge_lib=debug` output for the failing flow.
- **Protocol changes** — Xiaomi tweaks the auth flow occasionally. If you have a fresh HAR capture of the official desktop client, attach it to the issue.

Before opening a PR:

```bash
cd src-tauri && cargo fmt && cargo clippy -- -D warnings && cargo test
```

## Disclaimer

This project is an independent reverse-engineering effort intended for **educational and personal use**. It is not affiliated with, endorsed by, or sponsored by Xiaomi. By using miclaw_api_bridge you accept full responsibility for compliance with the Xiaomi terms of service applicable to your account. The authors provide no warranty and accept no liability.

## License

[MIT](LICENSE)
