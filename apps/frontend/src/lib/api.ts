const BASE_URL = process.env.NEXT_PUBLIC_API_URL ?? "http://localhost:8000";

export type PrivacyConfig = {
  mask_names: boolean;
  mask_emails: boolean;
  mask_phones: boolean;
  mask_ssn: boolean;
  mask_addresses: boolean;
  semantic_routing: boolean;
  rehydrate_responses: boolean;
  min_confidence_threshold: number;
};

export async function fetchConfig(): Promise<PrivacyConfig> {
  const res = await fetch(`${BASE_URL}/config`);
  if (!res.ok) throw new Error(`Failed to fetch config: ${res.status}`);
  return res.json();
}

export async function saveConfig(config: PrivacyConfig): Promise<PrivacyConfig> {
  const res = await fetch(`${BASE_URL}/config`, {
    method: "PUT",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(config),
  });
  if (!res.ok) throw new Error(`Failed to save config: ${res.status}`);
  return res.json();
}
