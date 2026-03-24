import { ApiKeyManager } from "@/components/api-key-manager";

export default function KeysPage() {
  return (
    <div className="flex flex-col gap-6 p-6">
      <div>
        <h1 className="text-xl font-semibold tracking-tight">API Keys</h1>
        <p className="text-sm text-muted-foreground">
          Manage keys used to authenticate requests through the MaskProxy proxy
        </p>
      </div>
      <ApiKeyManager />
    </div>
  );
}
