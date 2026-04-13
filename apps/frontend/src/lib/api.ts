// Relative path — Next.js rewrites /api/* → backend internally.
// Works both in Docker and local dev without any env var in the browser.
const BASE_URL = "/api";

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
