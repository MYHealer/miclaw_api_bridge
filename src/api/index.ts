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
  active_port: number | null;
  restart_required: boolean;
}

export interface ModelInfo {
  id: string;
  object: string;
  owned_by: string;
  family: string;
}

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const resp = await fetch(path, {
    ...init,
    headers: {
      "content-type": "application/json",
      ...(init?.headers ?? {}),
    },
  });
  const text = await resp.text();
  const data = text ? JSON.parse(text) : null;
  if (!resp.ok) {
    const message = data?.error?.message ?? `${resp.status} ${resp.statusText}`;
    throw new Error(message);
  }
  return data as T;
}

function post<T>(path: string, body?: unknown) {
  return request<T>(path, {
    method: "POST",
    body: JSON.stringify(body ?? {}),
  });
}

export const api = {
  authStatus: () => request<AuthSnapshot>("/api/auth/status"),
  login: (account: string, password: string, captcha?: string) =>
    post<LoginOutcome>("/api/auth/login", { account, password, captcha }),
  sendTicket: (flag: number) => post<boolean>("/api/auth/two-factor/send", { flag }),
  verifyTicket: (flag: number, ticket: string) =>
    post<void>("/api/auth/two-factor/verify", { flag, ticket }),
  refreshSession: () => post<AuthSnapshot>("/api/auth/refresh"),
  logout: () => post<void>("/api/auth/logout"),
  proxyStatus: () => request<ProxySnapshot>("/api/proxy/status"),
  setProxyPort: (port: number) => post<ProxySnapshot>("/api/settings/port", { port }),
  listModels: () => request<ModelInfo[]>("/api/models"),
};
