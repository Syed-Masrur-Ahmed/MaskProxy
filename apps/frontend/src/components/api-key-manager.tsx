"use client";

import { useEffect, useRef, useState } from "react";
import { Key, Plus, Trash2, Copy, Check, Eye, EyeOff } from "lucide-react";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Separator } from "@/components/ui/separator";
import { Badge } from "@/components/ui/badge";
import { useAuth } from "@/lib/auth";
import { listKeys, createKey, revokeKey, type APIKey, type APIKeyCreated } from "@/lib/api";

// ── New-key banner ────────────────────────────────────────────────────────────

function NewKeyBanner({
  created,
  onDismiss,
}: {
  created: APIKeyCreated;
  onDismiss: () => void;
}) {
  const [copied, setCopied] = useState(false);
  const [visible, setVisible] = useState(false);

  async function copy() {
    await navigator.clipboard.writeText(created.key);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  }

  return (
    <div className="rounded-md border border-emerald-500/40 bg-emerald-500/10 p-4 text-sm">
      <p className="font-medium text-emerald-700 dark:text-emerald-400">
        Key created — copy it now. You won&apos;t see it again.
      </p>
      <p className="mt-0.5 text-xs text-muted-foreground">{created.name}</p>
      <div className="mt-3 flex items-center gap-2">
        <code className="flex-1 truncate rounded bg-muted px-2 py-1 font-mono text-xs">
          {visible ? created.key : "mp_" + "•".repeat(40)}
        </code>
        <button
          onClick={() => setVisible((v) => !v)}
          className="shrink-0 text-muted-foreground transition-colors hover:text-foreground"
          aria-label={visible ? "Hide key" : "Reveal key"}
        >
          {visible ? <EyeOff className="h-4 w-4" /> : <Eye className="h-4 w-4" />}
        </button>
        <button
          onClick={copy}
          className="shrink-0 text-muted-foreground transition-colors hover:text-foreground"
          aria-label="Copy key"
        >
          {copied ? (
            <Check className="h-4 w-4 text-emerald-500" />
          ) : (
            <Copy className="h-4 w-4" />
          )}
        </button>
      </div>
      <button
        onClick={onDismiss}
        className="mt-3 text-xs text-muted-foreground underline-offset-2 hover:underline"
      >
        I&apos;ve saved it, dismiss
      </button>
    </div>
  );
}

// ── Create-key form ───────────────────────────────────────────────────────────

function CreateKeyForm({ onCreated }: { onCreated: (k: APIKeyCreated) => void }) {
  const { token } = useAuth();
  const [open, setOpen] = useState(false);
  const [name, setName] = useState("");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (open) inputRef.current?.focus();
  }, [open]);

  async function submit(e: React.FormEvent) {
    e.preventDefault();
    if (!token || !name.trim()) return;
    setLoading(true);
    setError(null);
    try {
      const created = await createKey(token, name.trim());
      onCreated(created);
      setName("");
      setOpen(false);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to create key");
    } finally {
      setLoading(false);
    }
  }

  if (!open) {
    return (
      <Button size="sm" onClick={() => setOpen(true)}>
        <Plus className="h-4 w-4" />
        New key
      </Button>
    );
  }

  return (
    <form onSubmit={submit} className="flex items-center gap-2">
      <input
        ref={inputRef}
        value={name}
        onChange={(e) => setName(e.target.value)}
        placeholder="Key name (e.g. Production)"
        className="h-9 flex-1 rounded-md border border-input bg-background px-3 text-sm outline-none ring-offset-background focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2"
        maxLength={64}
        disabled={loading}
      />
      <Button size="sm" type="submit" disabled={loading || !name.trim()}>
        {loading ? "Creating…" : "Create"}
      </Button>
      <Button
        size="sm"
        variant="ghost"
        type="button"
        onClick={() => { setOpen(false); setName(""); setError(null); }}
        disabled={loading}
      >
        Cancel
      </Button>
      {error && <span className="text-xs text-destructive">{error}</span>}
    </form>
  );
}

// ── Key row ───────────────────────────────────────────────────────────────────

function KeyRow({ apiKey, onRevoked }: { apiKey: APIKey; onRevoked: (id: string) => void }) {
  const { token } = useAuth();
  const [confirming, setConfirming] = useState(false);
  const [loading, setLoading] = useState(false);

  async function revoke() {
    if (!token) return;
    setLoading(true);
    try {
      await revokeKey(token, apiKey.id);
      onRevoked(apiKey.id);
    } finally {
      setLoading(false);
      setConfirming(false);
    }
  }

  const date = new Date(apiKey.created_at).toLocaleDateString(undefined, {
    year: "numeric",
    month: "short",
    day: "numeric",
  });

  return (
    <div className="flex items-center justify-between px-4 py-3">
      <div className="flex items-center gap-3 min-w-0">
        <Key className="h-4 w-4 shrink-0 text-muted-foreground" />
        <div className="min-w-0">
          <p className="truncate text-sm font-medium">{apiKey.name}</p>
          <p className="text-xs text-muted-foreground">
            <code className="font-mono">mp_…{apiKey.key_peek}</code>
            <span className="mx-1.5">·</span>
            {date}
          </p>
        </div>
      </div>

      <div className="ml-4 flex shrink-0 items-center gap-2">
        <Badge variant="secondary" className="text-xs">Active</Badge>
        {confirming ? (
          <>
            <Button size="sm" variant="destructive" onClick={revoke} disabled={loading}>
              {loading ? "Revoking…" : "Confirm"}
            </Button>
            <Button size="sm" variant="ghost" onClick={() => setConfirming(false)} disabled={loading}>
              Cancel
            </Button>
          </>
        ) : (
          <Button
            size="sm"
            variant="ghost"
            onClick={() => setConfirming(true)}
            aria-label="Revoke key"
          >
            <Trash2 className="h-4 w-4 text-muted-foreground" />
          </Button>
        )}
      </div>
    </div>
  );
}

// ── Main component ────────────────────────────────────────────────────────────

export function ApiKeyManager() {
  const { token } = useAuth();
  const [keys, setKeys] = useState<APIKey[]>([]);
  const [newKey, setNewKey] = useState<APIKeyCreated | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!token) return;
    listKeys(token)
      .then(setKeys)
      .catch((e) => setError(e.message))
      .finally(() => setLoading(false));
  }, [token]);

  function handleCreated(created: APIKeyCreated) {
    setNewKey(created);
    setKeys((prev) => [
      { id: created.id, name: created.name, key_peek: created.key_peek, created_at: created.created_at },
      ...prev,
    ]);
  }

  function handleRevoked(id: string) {
    setKeys((prev) => prev.filter((k) => k.id !== id));
    if (newKey?.id === id) setNewKey(null);
  }

  return (
    <Card>
      <CardHeader className="pb-3">
        <div className="flex items-start justify-between gap-4">
          <div>
            <CardTitle className="flex items-center gap-2 text-base">
              <Key className="h-4 w-4 text-primary" />
              API Keys
            </CardTitle>
            <CardDescription className="mt-1">
              {loading
                ? "Loading…"
                : error
                  ? "Failed to load keys"
                  : `${keys.length} key${keys.length === 1 ? "" : "s"} — used to authenticate proxy requests`}
            </CardDescription>
          </div>
          <CreateKeyForm onCreated={handleCreated} />
        </div>
      </CardHeader>

      <CardContent className="space-y-3 p-4 pt-0">
        {error && <p className="text-xs text-destructive">{error}</p>}

        {newKey && (
          <NewKeyBanner created={newKey} onDismiss={() => setNewKey(null)} />
        )}

        {!loading && keys.length === 0 && !error && (
          <p className="py-6 text-center text-sm text-muted-foreground">
            No API keys yet. Create one to start proxying requests.
          </p>
        )}

        {keys.length > 0 && (
          <div className="rounded-md border">
            {keys.map((k, i) => (
              <div key={k.id}>
                {i > 0 && <Separator />}
                <KeyRow apiKey={k} onRevoked={handleRevoked} />
              </div>
            ))}
          </div>
        )}
      </CardContent>
    </Card>
  );
}
