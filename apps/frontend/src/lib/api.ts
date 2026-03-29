// Relative path — Next.js rewrites /api/* → backend internally.
// Works both in Docker and local dev without any env var in the browser.
const BASE_URL = "/api";

// ── User Management ───────────────────────────────────────────────────────────

export type UserProfile = {
  email: string;
  created_at: string;
};

export async function getMe(token: string): Promise<UserProfile> {
  const res = await fetch(`${BASE_URL}/users/me`, { headers: authHeaders(token) });
  if (!res.ok) throw new Error(`Failed to fetch profile: ${res.status}`);
  return res.json();
}

export async function updateEmail(token: string, email: string): Promise<UserProfile> {
  const res = await fetch(`${BASE_URL}/users/me`, {
    method: "PATCH",
    headers: authHeaders(token),
    body: JSON.stringify({ email }),
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
  const res = await fetch(`${BASE_URL}/users/me/password`, {
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
  const res = await fetch(`${BASE_URL}/config`, {
    headers: authHeaders(token),
  });
  if (!res.ok) throw new Error(`Failed to fetch config: ${res.status}`);
  return res.json();
}

export async function saveConfig(
  token: string,
  patch: PrivacyConfigUpdate,
): Promise<PrivacyConfig> {
  const res = await fetch(`${BASE_URL}/config`, {
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
  const res = await fetch(`${BASE_URL}/keys`, { headers: authHeaders(token) });
  if (!res.ok) throw new Error(`Failed to fetch keys: ${res.status}`);
  return res.json();
}

export async function createKey(token: string, name: string): Promise<APIKeyCreated> {
  const res = await fetch(`${BASE_URL}/keys`, {
    method: "POST",
    headers: authHeaders(token),
    body: JSON.stringify({ name }),
  });
  if (!res.ok) throw new Error(`Failed to create key: ${res.status}`);
  return res.json();
}

export async function revokeKey(token: string, keyId: string): Promise<void> {
  const res = await fetch(`${BASE_URL}/keys/${keyId}`, {
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
  const res = await fetch(`${BASE_URL}/v1/provider-keys`, { headers: authHeaders(token) });
  if (!res.ok) throw new Error(`Failed to fetch provider keys: ${res.status}`);
  return res.json();
}

export async function addProviderKey(
  token: string,
  provider_name: string,
  raw_key: string,
): Promise<ProviderKey> {
  const res = await fetch(`${BASE_URL}/v1/provider-keys`, {
    method: "POST",
    headers: authHeaders(token),
    body: JSON.stringify({ provider_name, raw_key }),
  });
  if (!res.ok) throw new Error(`Failed to add provider key: ${res.status}`);
  return res.json();
}

export async function deleteProviderKey(token: string, keyId: string): Promise<void> {
  const res = await fetch(`${BASE_URL}/v1/provider-keys/${keyId}`, {
    method: "DELETE",
    headers: authHeaders(token),
  });
  if (!res.ok) throw new Error(`Failed to delete provider key: ${res.status}`);
}
