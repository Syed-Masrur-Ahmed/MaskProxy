"use client";

import { useState } from "react";
import { User, Shield } from "lucide-react";
import { cn } from "@/lib/utils";
import { AccountSettings } from "./account-settings";
import { RoutingSettings } from "./routing-settings";

const TABS = [
  { id: "account", label: "User Management", icon: User },
  { id: "routing", label: "Routing Rules", icon: Shield },
] as const;

type TabId = (typeof TABS)[number]["id"];

export default function SettingsPage() {
  const [activeTab, setActiveTab] = useState<TabId>("account");

  return (
    <div className="flex flex-col gap-6 p-6">
      <div>
        <h1 className="text-xl font-semibold tracking-tight">Settings</h1>
        <p className="text-sm text-muted-foreground">Manage your account and proxy configuration</p>
      </div>

      {/* Tab bar */}
      <div className="flex gap-1 border-b">
        {TABS.map(({ id, label, icon: Icon }) => (
          <button
            key={id}
            onClick={() => setActiveTab(id)}
            className={cn(
              "flex items-center gap-2 px-4 py-2.5 text-sm font-medium transition-colors border-b-2 -mb-px",
              activeTab === id
                ? "border-primary text-foreground"
                : "border-transparent text-muted-foreground hover:text-foreground",
            )}
          >
            <Icon className="h-4 w-4" />
            {label}
          </button>
        ))}
      </div>

      {/* Tab content */}
      {activeTab === "account" && <AccountSettings />}
      {activeTab === "routing" && <RoutingSettings />}
    </div>
  );
}
