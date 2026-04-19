// Relative path — Next.js rewrites /api/* → backend internally.
// Works both in Docker and local dev without any env var in the browser.
const BASE_URL = "/api";

let _onUnauthorized: (() => void) | null = null;

export function setUnauthorizedHandler(fn: () => void): void {
  _onUnauthorized = fn;
}

async function apiFetch(
  input: RequestInfo | URL,
  init?: RequestInit,
): Promise<Response> {
  const res = await fetch(input, init);
  if (res.status === 401) {
    _onUnauthorized?.();
    throw new Error("Session expired. Please log in again.");
  }
  return res;
}

// ── User Management ───────────────────────────────────────────────────────────

export type UserProfile = {
  email: string;
  created_at: string;
  access_token?: string;
};

export async function getMe(token: string): Promise<UserProfile> {
  const res = await apiFetch(`${BASE_URL}/users/me`, { headers: authHeaders(token) });
  if (!res.ok) throw new Error(`Failed to fetch profile: ${res.status}`);
  return res.json();
}

export async function updateEmail(token: string, email: string, password: string): Promise<UserProfile> {
  const res = await apiFetch(`${BASE_URL}/users/me`, {
    method: "PATCH",
    headers: authHeaders(token),
    body: JSON.stringify({ email, password }),
  });
  if (!res.ok) {
    const body = await res.json().catch(() => ({}));
    throw new Error(body.detail ?? `Failed to update email: ${res.status}`);
  }
  return res.json();
}

export async function updatePassword(
  token: string,
  current_password: string,
  new_password: string,
): Promise<void> {
  const res = await apiFetch(`${BASE_URL}/users/me/password`, {
    method: "PATCH",
    headers: authHeaders(token),
    body: JSON.stringify({ current_password, new_password }),
  });
  if (!res.ok) {
    const body = await res.json().catch(() => ({}));
    throw new Error(body.detail ?? `Failed to update password: ${res.status}`);
  }
}

export type PrivacyConfig = {
  mask_names: boolean;
  mask_locations: boolean;
  mask_finance: boolean;
  threshold: number;
};

export type PrivacyConfigUpdate = Partial<PrivacyConfig>;

function authHeaders(token: string) {
  return {
    "Content-Type": "application/json",
    Authorization: `Bearer ${token}`,
  };
}

export async function fetchConfig(token: string): Promise<PrivacyConfig> {
  const res = await apiFetch(`${BASE_URL}/config`, {
    headers: authHeaders(token),
  });
  if (!res.ok) throw new Error(`Failed to fetch config: ${res.status}`);
  return res.json();
}

export async function saveConfig(
  token: string,
  patch: PrivacyConfigUpdate,
): Promise<PrivacyConfig> {
  const res = await apiFetch(`${BASE_URL}/config`, {
    method: "PATCH",
    headers: authHeaders(token),
    body: JSON.stringify(patch),
  });
  if (!res.ok) throw new Error(`Failed to save config: ${res.status}`);
  return res.json();
}

// ── API Keys ──────────────────────────────────────────────────────────────────

export type APIKey = {
  id: string;
  name: string;
  key_peek: string;
  created_at: string;
};

export type APIKeyCreated = APIKey & { key: string };

export async function listKeys(token: string): Promise<APIKey[]> {
  const res = await apiFetch(`${BASE_URL}/keys`, { headers: authHeaders(token) });
  if (!res.ok) throw new Error(`Failed to fetch keys: ${res.status}`);
  return res.json();
}

export async function createKey(token: string, name: string): Promise<APIKeyCreated> {
  const res = await apiFetch(`${BASE_URL}/keys`, {
    method: "POST",
    headers: authHeaders(token),
    body: JSON.stringify({ name }),
  });
  if (!res.ok) throw new Error(`Failed to create key: ${res.status}`);
  return res.json();
}

export async function revokeKey(token: string, keyId: string): Promise<void> {
  const res = await apiFetch(`${BASE_URL}/keys/${keyId}`, {
    method: "DELETE",
    headers: authHeaders(token),
  });
  if (!res.ok) throw new Error(`Failed to revoke key: ${res.status}`);
}

// ── Provider Keys (Vault) ──────────────────────────────────────────────────────

export type ProviderKey = {
  id: string;
  provider_name: string;
  key_peek: string;
  created_at: string;
};

export async function listProviderKeys(token: string): Promise<ProviderKey[]> {
  const res = await apiFetch(`${BASE_URL}/v1/provider-keys`, { headers: authHeaders(token) });
  if (!res.ok) throw new Error(`Failed to fetch provider keys: ${res.status}`);
  return res.json();
}

export async function addProviderKey(
  token: string,
  provider_name: string,
  raw_key: string,
): Promise<ProviderKey> {
  const res = await apiFetch(`${BASE_URL}/v1/provider-keys`, {
    method: "POST",
    headers: authHeaders(token),
    body: JSON.stringify({ provider_name, raw_key }),
  });
  if (!res.ok) throw new Error(`Failed to add provider key: ${res.status}`);
  return res.json();
}

export async function deleteProviderKey(token: string, keyId: string): Promise<void> {
  const res = await apiFetch(`${BASE_URL}/v1/provider-keys/${keyId}`, {
    method: "DELETE",
    headers: authHeaders(token),
  });
  if (!res.ok) throw new Error(`Failed to delete provider key: ${res.status}`);
}

// ── Request Logs ─────────────────────────────────────────────────────────────

export type LogEntry = {
  id: string;
  session_id: string;
  timestamp: string;
  provider: string;
  model: string;
  pii_detected_count: number;
  pii_types: string[];
  route: string;
  latency_ms: number;
  masked_prompt: string;
  status_code: number;
};

export type DashboardStats = {
  requests_today: number;
  pii_entities_masked: number;
  avg_latency_ms: number;
  local_route_pct: number;
};

export async function fetchLogs(
  token: string,
  opts: { limit?: number; offset?: number } = {},
): Promise<LogEntry[]> {
  const limit = opts.limit ?? 50;
  const offset = opts.offset ?? 0;
  const res = await apiFetch(
    `${BASE_URL}/v1/logs?limit=${limit}&offset=${offset}`,
    { headers: authHeaders(token) },
  );
  if (!res.ok) throw new Error(`Failed to fetch logs: ${res.status}`);
  return res.json();
}

export async function fetchStats(token: string): Promise<DashboardStats> {
  const res = await apiFetch(`${BASE_URL}/v1/logs/stats`, {
    headers: authHeaders(token),
  });
  if (!res.ok) throw new Error(`Failed to fetch stats: ${res.status}`);
  return res.json();
}
