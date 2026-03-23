"use client";

import { useEffect, useState } from "react";
import { Switch } from "@/components/ui/switch";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Separator } from "@/components/ui/separator";
import { Shield } from "lucide-react";
import { fetchConfig, saveConfig, type PrivacyConfig } from "@/lib/api";
import { useAuth } from "@/lib/auth";

type BoolKey = "mask_names" | "mask_locations" | "mask_finance";

const SETTINGS_META: { id: BoolKey; label: string; description: string }[] = [
  {
    id: "mask_names",
    label: "Mask Person Names",
    description: "Replace detected names with [PERSON_N] tokens",
  },
  {
    id: "mask_locations",
    label: "Mask Locations",
    description: "Replace location names with [LOCATION_N] tokens",
  },
  {
    id: "mask_finance",
    label: "Mask Financial Data",
    description: "Replace financial identifiers with [FINANCE_N] tokens",
  },
];

export function PrivacySettings() {
  const { token } = useAuth();
  const [config, setConfig] = useState<PrivacyConfig | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!token) return;
    fetchConfig(token)
      .then(setConfig)
      .catch((e) => setError(e.message));
  }, [token]);

  async function toggle(id: BoolKey) {
    if (!config || !token) return;
    const patch = { [id]: !config[id] };
    setConfig({ ...config, ...patch });
    try {
      const saved = await saveConfig(token, patch);
      setConfig(saved);
    } catch (e) {
      setConfig(config);
      setError(e instanceof Error ? e.message : "Failed to save");
    }
  }

  async function commitThreshold(value: number) {
    if (!config || !token) return;
    try {
      const saved = await saveConfig(token, { threshold: value });
      setConfig(saved);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to save");
    }
  }

  const enabledCount = config ? SETTINGS_META.filter((s) => config[s.id]).length : 0;

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
                checked={config ? config[setting.id] : false}
                onCheckedChange={() => toggle(setting.id)}
                disabled={!config}
                aria-label={setting.label}
              />
            </div>
          </div>
        ))}
        <Separator className="mx-4 w-auto" />
        <div className="flex flex-col gap-2 px-4 py-3">
          <div className="flex items-center justify-between">
            <p className="text-sm font-medium leading-none">Detection Threshold</p>
            <span className="text-xs tabular-nums text-muted-foreground">
              {config ? config.threshold.toFixed(2) : "—"}
            </span>
          </div>
          <p className="text-xs text-muted-foreground">
            Minimum NER confidence required to mask an entity
          </p>
          <input
            type="range"
            min="0"
            max="1"
            step="0.05"
            value={config?.threshold ?? 0.75}
            onChange={(e) =>
              config && setConfig({ ...config, threshold: parseFloat(e.target.value) })
            }
            onMouseUp={(e) => commitThreshold(parseFloat((e.target as HTMLInputElement).value))}
            onTouchEnd={(e) => commitThreshold(parseFloat((e.target as HTMLInputElement).value))}
            disabled={!config}
            className="w-full accent-primary disabled:opacity-50"
          />
        </div>
      </CardContent>
    </Card>
  );
}
