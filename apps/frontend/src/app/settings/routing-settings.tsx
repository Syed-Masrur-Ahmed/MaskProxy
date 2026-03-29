"use client";

import { useEffect, useState } from "react";
import { Switch } from "@/components/ui/switch";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Separator } from "@/components/ui/separator";
import { fetchConfig, saveConfig, type PrivacyConfig } from "@/lib/api";
import { useAuth } from "@/lib/auth";
import { useToast } from "@/hooks/use-toast";

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

export function RoutingSettings() {
  const { token } = useAuth();
  const { toast } = useToast();
  const [config, setConfig] = useState<PrivacyConfig | null>(null);

  useEffect(() => {
    if (!token) return;
    fetchConfig(token)
      .then(setConfig)
      .catch((e) =>
        toast({ title: "Failed to load config", description: e.message, variant: "destructive" }),
      );
  }, [token]);

  async function toggle(id: BoolKey) {
    if (!config || !token) return;
    const prev = config;
    const patch = { [id]: !config[id] };
    setConfig({ ...config, ...patch });
    try {
      const saved = await saveConfig(token, patch);
      setConfig(saved);
    } catch (e) {
      setConfig(prev);
      toast({
        title: "Failed to save",
        description: e instanceof Error ? e.message : "Unknown error",
        variant: "destructive",
      });
    }
  }

  async function commitThreshold(value: number) {
    if (!config || !token) return;
    try {
      const saved = await saveConfig(token, { threshold: value });
      setConfig(saved);
    } catch (e) {
      toast({
        title: "Failed to save threshold",
        description: e instanceof Error ? e.message : "Unknown error",
        variant: "destructive",
      });
    }
  }

  const enabledCount = config ? SETTINGS_META.filter((s) => config[s.id]).length : 0;

  return (
    <div className="flex flex-col gap-6 max-w-lg">
      <Card>
        <CardHeader>
          <CardTitle className="text-base">PII Detection</CardTitle>
          <CardDescription>
            {config
              ? `${enabledCount} of ${SETTINGS_META.length} protections active`
              : "Loading…"}
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-1 p-0 pb-2">
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
    </div>
  );
}
