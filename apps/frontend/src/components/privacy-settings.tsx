"use client";

import { useEffect, useState } from "react";
import { Switch } from "@/components/ui/switch";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Separator } from "@/components/ui/separator";
import { Shield } from "lucide-react";
import { fetchConfig, saveConfig, type PrivacyConfig } from "@/lib/api";

type SettingMeta = {
  id: keyof PrivacyConfig;
  label: string;
  description: string;
};

const SETTINGS_META: SettingMeta[] = [
  {
    id: "mask_names",
    label: "Mask Person Names",
    description: "Replace detected names with [PERSON_N] tokens",
  },
  {
    id: "mask_emails",
    label: "Mask Email Addresses",
    description: "Replace email addresses with [EMAIL_N] tokens",
  },
  {
    id: "mask_phones",
    label: "Mask Phone Numbers",
    description: "Replace phone numbers with [PHONE_N] tokens",
  },
  {
    id: "mask_ssn",
    label: "Mask SSNs",
    description: "Replace Social Security Numbers with [SSN_N] tokens",
  },
  {
    id: "mask_addresses",
    label: "Mask Addresses",
    description: "Replace physical addresses with [ADDRESS_N] tokens",
  },
  {
    id: "semantic_routing",
    label: "Semantic Routing",
    description: "Route sensitive prompts to local inference instead of cloud LLMs",
  },
  {
    id: "rehydrate_responses",
    label: "Rehydrate Responses",
    description: "Replace placeholder tokens with real values in LLM responses",
  },
];

export function PrivacySettings() {
  const [config, setConfig] = useState<PrivacyConfig | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    fetchConfig()
      .then(setConfig)
      .catch((e) => setError(e.message));
  }, []);

  async function toggle(id: keyof PrivacyConfig) {
    if (!config) return;
    const next = { ...config, [id]: !config[id] };
    setConfig(next);
    try {
      const saved = await saveConfig(next);
      setConfig(saved);
    } catch (e) {
      setConfig(config); // revert on failure
      setError(e instanceof Error ? e.message : "Failed to save");
    }
  }

  const enabledCount = config
    ? SETTINGS_META.filter((s) => config[s.id]).length
    : 0;

  return (
    <Card>
      <CardHeader className="pb-3">
        <CardTitle className="flex items-center gap-2 text-base">
          <Shield className="h-4 w-4 text-primary" />
          Privacy Settings
        </CardTitle>
        <CardDescription>
          {config
            ? `${enabledCount} of ${SETTINGS_META.length} protections active`
            : error
              ? "Failed to load settings"
              : "Loading…"}
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-1 p-0 pb-2">
        {error && (
          <p className="px-4 py-2 text-xs text-destructive">{error}</p>
        )}
        {SETTINGS_META.map((setting, i) => (
          <div key={setting.id}>
            {i > 0 && <Separator className="mx-4 w-auto" />}
            <div className="flex items-center justify-between px-4 py-3">
              <div className="space-y-0.5">
                <p className="text-sm font-medium leading-none">{setting.label}</p>
                <p className="text-xs text-muted-foreground">{setting.description}</p>
              </div>
              <Switch
                checked={config ? !!config[setting.id] : false}
                onCheckedChange={() => toggle(setting.id)}
                disabled={!config}
                aria-label={setting.label}
              />
            </div>
          </div>
        ))}
      </CardContent>
    </Card>
  );
}
