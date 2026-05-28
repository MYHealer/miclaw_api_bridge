import { invoke } from "@tauri-apps/api/core";

export interface AuthSnapshot {
  authenticated: boolean;
  nick: string | null;
  user_id: string | null;
  refreshed_at: number | null;
}

export type LoginOutcome =
  | { outcome: "authenticated"; nick: string | null }
  | { outcome: "two_factor_required"; options: number[] }
  | { outcome: "captcha_required"; captcha_url: string }
  | { outcome: "failed"; code: number; description: string };

export interface ProxySnapshot {
  running: boolean;
  addr: string | null;
  port: number;
}

export interface ModelInfo {
  id: string;
  object: string;
  owned_by: string;
  family: string;
}

const previewModels: ModelInfo[] = [
  { id: "mimo-omni", object: "model", owned_by: "xiaomi", family: "chat (multimodal, 256K)" },
  { id: "mimo-pro", object: "model", owned_by: "xiaomi", family: "chat (reasoning)" },
  { id: "mimo-pro-1m", object: "model", owned_by: "xiaomi", family: "chat (reasoning, 1M context)" },
  { id: "xiaomi/mimo-v2-pro", object: "model", owned_by: "xiaomi", family: "chat (v2 reasoning)" },
];

function inTauri() {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

async function tauriOrPreview<T>(command: string, args: Record<string, unknown>, preview: T) {
  if (!inTauri()) return preview;
  return invoke<T>(command, args);
}

export const api = {
  authStatus: () =>
    tauriOrPreview<AuthSnapshot>("auth_status", {}, {
      authenticated: false,
      nick: null,
      user_id: null,
      refreshed_at: null,
    }),
  login: (account: string, password: string, captcha?: string) =>
    tauriOrPreview<LoginOutcome>(
      "login",
      { req: { account, password, captcha } },
      { outcome: "two_factor_required", options: [8, 4] },
    ),
  sendTicket: (flag: number) => tauriOrPreview<boolean>("send_two_factor_ticket", { flag }, true),
  verifyTicket: (flag: number, ticket: string) =>
    tauriOrPreview<void>("verify_two_factor", { flag, ticket }, undefined),
  refreshSession: () =>
    tauriOrPreview<AuthSnapshot>("refresh_session", {}, {
      authenticated: true,
      nick: "preview",
      user_id: "preview",
      refreshed_at: Date.now(),
    }),
  logout: () => tauriOrPreview<void>("logout", {}, undefined),
  proxyStatus: () =>
    tauriOrPreview<ProxySnapshot>("proxy_status", {}, {
      running: true,
      addr: "127.0.0.1:8765",
      port: 8765,
    }),
  startProxy: () =>
    tauriOrPreview<ProxySnapshot>("start_proxy", {}, {
      running: true,
      addr: "127.0.0.1:8765",
      port: 8765,
    }),
  stopProxy: () =>
    tauriOrPreview<ProxySnapshot>("stop_proxy", {}, {
      running: false,
      addr: null,
      port: 8765,
    }),
  setProxyPort: (port: number) =>
    tauriOrPreview<ProxySnapshot>("set_proxy_port", { port }, {
      running: true,
      addr: `127.0.0.1:${port}`,
      port,
    }),
  listModels: () => tauriOrPreview<ModelInfo[]>("list_models", {}, previewModels),
};
