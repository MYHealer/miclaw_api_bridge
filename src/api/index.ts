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

export const api = {
  authStatus: () => invoke<AuthSnapshot>("auth_status"),
  login: (account: string, password: string, captcha?: string) =>
    invoke<LoginOutcome>("login", { req: { account, password, captcha } }),
  sendTicket: (flag: number) => invoke<boolean>("send_two_factor_ticket", { flag }),
  verifyTicket: (flag: number, ticket: string) =>
    invoke<void>("verify_two_factor", { flag, ticket }),
  refreshSession: () => invoke<AuthSnapshot>("refresh_session"),
  logout: () => invoke<void>("logout"),
  proxyStatus: () => invoke<ProxySnapshot>("proxy_status"),
  startProxy: () => invoke<ProxySnapshot>("start_proxy"),
  stopProxy: () => invoke<ProxySnapshot>("stop_proxy"),
  setProxyPort: (port: number) => invoke<ProxySnapshot>("set_proxy_port", { port }),
  listModels: () => invoke<ModelInfo[]>("list_models"),
};
