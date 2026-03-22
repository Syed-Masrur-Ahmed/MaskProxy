"use client";

import { useState } from "react";
import { Switch } from "@/components/ui/switch";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Separator } from "@/components/ui/separator";
import { Shield } from "lucide-react";

type PrivacySetting = {
  id: string;
  label: string;
  description: string;
  defaultEnabled: boolean;
};

const PRIVACY_SETTINGS: PrivacySetting[] = [
  {
    id: "mask_names",
    label: "Mask Person Names",
    description: "Replace detected names with [PERSON_N] tokens",
    defaultEnabled: true,
  },
  {
    id: "mask_emails",
    label: "Mask Email Addresses",
    description: "Replace email addresses with [EMAIL_N] tokens",
    defaultEnabled: true,
  },
  {
    id: "mask_phones",
    label: "Mask Phone Numbers",
    description: "Replace phone numbers with [PHONE_N] tokens",
    defaultEnabled: true,
  },
  {
    id: "mask_ssn",
    label: "Mask SSNs",
    description: "Replace Social Security Numbers with [SSN_N] tokens",
    defaultEnabled: true,
  },
  {
    id: "mask_addresses",
    label: "Mask Addresses",
    description: "Replace physical addresses with [ADDRESS_N] tokens",
    defaultEnabled: false,
  },
  {
    id: "semantic_routing",
    label: "Semantic Routing",
    description: "Route sensitive prompts to local inference instead of cloud LLMs",
    defaultEnabled: false,
  },
  {
    id: "rehydrate_responses",
    label: "Rehydrate Responses",
    description: "Replace placeholder tokens with real values in LLM responses",
    defaultEnabled: true,
  },
];

export function PrivacySettings() {
  const [settings, setSettings] = useState<Record<string, boolean>>(
    Object.fromEntries(PRIVACY_SETTINGS.map((s) => [s.id, s.defaultEnabled]))
  );

  const toggle = (id: string) =>
    setSettings((prev) => ({ ...prev, [id]: !prev[id] }));

  const enabledCount = Object.values(settings).filter(Boolean).length;

  return (
    <Card>
      <CardHeader className="pb-3">
        <CardTitle className="flex items-center gap-2 text-base">
          <Shield className="h-4 w-4 text-primary" />
          Privacy Settings
        </CardTitle>
        <CardDescription>
          {enabledCount} of {PRIVACY_SETTINGS.length} protections active
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-1 p-0 pb-2">
        {PRIVACY_SETTINGS.map((setting, i) => (
          <div key={setting.id}>
            {i > 0 && <Separator className="mx-4 w-auto" />}
            <div className="flex items-center justify-between px-4 py-3">
              <div className="space-y-0.5">
                <p className="text-sm font-medium leading-none">{setting.label}</p>
                <p className="text-xs text-muted-foreground">{setting.description}</p>
              </div>
              <Switch
                checked={settings[setting.id]}
                onCheckedChange={() => toggle(setting.id)}
                aria-label={setting.label}
              />
            </div>
          </div>
        ))}
      </CardContent>
    </Card>
  );
}
