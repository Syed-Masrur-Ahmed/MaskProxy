"use client";

import * as React from "react";
import { CheckCircle2, Plug, Trash2, KeyRound } from "lucide-react";

import { useAuth } from "@/lib/auth";
import {
  listProviderKeys,
  addProviderKey,
  deleteProviderKey,
  type ProviderKey,
} from "@/lib/api";
import { toast } from "@/hooks/use-toast";

import { Card, CardContent, CardHeader, CardTitle, CardDescription } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogClose,
  DialogCloseProvider,
} from "@/components/ui/dialog";
import { cn } from "@/lib/utils";

// ── Supported providers ────────────────────────────────────────────────────────

type Provider = { name: string; description: string };

const PROVIDERS: Provider[] = [
  { name: "OpenAI", description: "GPT-4o, GPT-4 Turbo, and o1 models" },
  { name: "Anthropic", description: "Claude 3.5 Sonnet, Haiku, and Opus" },
  { name: "Gemini", description: "Gemini 2.0 Flash, Pro, and Ultra models" },
];

// ── Page ──────────────────────────────────────────────────────────────────────

export default function ProviderVaultPage() {
  const { token } = useAuth();

  const [connectedKeys, setConnectedKeys] = React.useState<ProviderKey[]>([]);
  const [isLoading, setIsLoading] = React.useState(true);

  const [dialogOpen, setDialogOpen] = React.useState(false);
  const [selectedProvider, setSelectedProvider] = React.useState(PROVIDERS[0].name);
  const [rawKey, setRawKey] = React.useState("");
  const [isSaving, setIsSaving] = React.useState(false);
  const [revokingId, setRevokingId] = React.useState<string | null>(null);

  // ── Fetch ──────────────────────────────────────────────────────────────────

  async function fetchKeys() {
    if (!token) return;
    try {
      const keys = await listProviderKeys(token);
      setConnectedKeys(keys);
    } catch (err) {
      toast({ title: "Failed to load keys", description: (err as Error).message, variant: "destructive" });
    } finally {
      setIsLoading(false);
    }
  }

  React.useEffect(() => {
    fetchKeys();
  }, [token]); // eslint-disable-line react-hooks/exhaustive-deps

  const connectedMap = React.useMemo(
    () => new Map(connectedKeys.map((k) => [k.provider_name, k])),
    [connectedKeys]
  );

  // ── Handlers ──────────────────────────────────────────────────────────────

  function openConnectDialog(providerName: string) {
    setSelectedProvider(providerName);
    setRawKey("");
    setDialogOpen(true);
  }

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    if (!rawKey.trim() || !token) return;
    setIsSaving(true);
    try {
      await addProviderKey(token, selectedProvider, rawKey.trim());
      setRawKey("");
      setDialogOpen(false);
      toast({ title: "Key saved", description: `${selectedProvider} connected successfully.` });
      fetchKeys();
    } catch (err) {
      toast({ title: "Failed to save key", description: (err as Error).message, variant: "destructive" });
    } finally {
      setIsSaving(false);
    }
  }

  async function handleRevoke(keyId: string, providerName: string) {
    if (!token) return;
    setRevokingId(keyId);
    try {
      await deleteProviderKey(token, keyId);
      toast({ title: "Key removed", description: `${providerName} disconnected.` });
      fetchKeys();
    } catch (err) {
      toast({ title: "Failed to remove key", description: (err as Error).message, variant: "destructive" });
    } finally {
      setRevokingId(null);
    }
  }

  // ── Render ────────────────────────────────────────────────────────────────

  return (
    <div className="p-8">
      {/* Header */}
      <div className="mb-8">
        <h1 className="text-2xl font-semibold tracking-tight">Provider Vault</h1>
        <p className="mt-1 text-sm text-muted-foreground">
          Securely store your LLM provider API keys. Keys are encrypted at rest and never returned
          after saving.
        </p>
      </div>

      {/* Provider grid */}
      <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-3">
        {PROVIDERS.map((provider) => {
          const connected = connectedMap.get(provider.name);
          return (
            <ProviderCard
              key={provider.name}
              provider={provider}
              connectedKey={connected}
              isLoading={isLoading}
              onConnect={() => openConnectDialog(provider.name)}
              onRevoke={(id) => handleRevoke(id, provider.name)}
              isRevoking={revokingId === connected?.id}
            />
          );
        })}
      </div>

      {/* Connect dialog */}
      <Dialog open={dialogOpen} onOpenChange={setDialogOpen}>
        <DialogCloseProvider onClose={() => setDialogOpen(false)}>
          <DialogContent>
            <DialogClose />
            <DialogHeader>
              <DialogTitle>Connect {selectedProvider}</DialogTitle>
              <DialogDescription>
                Your key is encrypted before being stored. It cannot be retrieved after saving.
              </DialogDescription>
            </DialogHeader>

            <form onSubmit={handleSubmit} className="mt-2 space-y-4">
              <div className="space-y-1.5">
                <label className="text-sm font-medium" htmlFor="provider-select">
                  Provider
                </label>
                <select
                  id="provider-select"
                  value={selectedProvider}
                  onChange={(e) => setSelectedProvider(e.target.value)}
                  className="w-full rounded-md border bg-background px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-ring"
                >
                  {PROVIDERS.map((p) => (
                    <option key={p.name} value={p.name}>
                      {p.name}
                    </option>
                  ))}
                </select>
              </div>

              <div className="space-y-1.5">
                <label className="text-sm font-medium" htmlFor="api-key-input">
                  API Key
                </label>
                <input
                  id="api-key-input"
                  type="password"
                  autoComplete="off"
                  placeholder="sk-••••••••••••••••"
                  value={rawKey}
                  onChange={(e) => setRawKey(e.target.value)}
                  className="w-full rounded-md border bg-background px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-ring"
                />
              </div>

              <div className="flex justify-end gap-2 pt-2">
                <DialogClose asChild>
                  <Button type="button" variant="outline" size="sm">
                    Cancel
                  </Button>
                </DialogClose>
                <Button type="submit" size="sm" disabled={!rawKey.trim() || isSaving}>
                  {isSaving ? "Saving…" : "Save Key"}
                </Button>
              </div>
            </form>
          </DialogContent>
        </DialogCloseProvider>
      </Dialog>
    </div>
  );
}

// ── ProviderCard ──────────────────────────────────────────────────────────────

type ProviderCardProps = {
  provider: Provider;
  connectedKey: ProviderKey | undefined;
  isLoading: boolean;
  onConnect: () => void;
  onRevoke: (id: string) => void;
  isRevoking: boolean;
};

function ProviderCard({
  provider,
  connectedKey,
  isLoading,
  onConnect,
  onRevoke,
  isRevoking,
}: ProviderCardProps) {
  const isConnected = !!connectedKey;

  return (
    <Card className={cn("transition-shadow", isConnected && "border-emerald-500/40")}>
      <CardHeader className="pb-3">
        <div className="flex items-start justify-between gap-2">
          <div className="flex items-center gap-2">
            <KeyRound className="h-4 w-4 shrink-0 text-muted-foreground" />
            <CardTitle className="text-base">{provider.name}</CardTitle>
          </div>
          {isConnected && (
            <Badge variant="success" className="shrink-0">
              <CheckCircle2 className="mr-1 h-3 w-3" />
              Connected
            </Badge>
          )}
        </div>
        <CardDescription>{provider.description}</CardDescription>
      </CardHeader>

      <CardContent className="pt-0">
        {isLoading ? (
          <div className="h-8 w-full animate-pulse rounded-md bg-muted" />
        ) : isConnected ? (
          <div className="flex items-center justify-between">
            <span className="font-mono text-sm text-muted-foreground">
              {"••••••••"}
              {connectedKey.key_peek}
            </span>
            <Button
              variant="ghost"
              size="sm"
              className="h-7 px-2 text-destructive hover:bg-destructive/10 hover:text-destructive"
              onClick={() => onRevoke(connectedKey.id)}
              disabled={isRevoking}
            >
              <Trash2 className="mr-1.5 h-3.5 w-3.5" />
              {isRevoking ? "Removing…" : "Revoke"}
            </Button>
          </div>
        ) : (
          <Button variant="outline" size="sm" className="w-full" onClick={onConnect}>
            <Plug className="mr-2 h-3.5 w-3.5" />
            Connect
          </Button>
        )}
      </CardContent>
    </Card>
  );
}
