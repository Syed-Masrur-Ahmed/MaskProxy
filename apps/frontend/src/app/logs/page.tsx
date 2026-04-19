import { LogsTable } from "@/components/logs-table";

export default function LogsPage() {
  return (
    <div className="flex flex-col gap-6 p-6">
      <div>
        <h1 className="text-xl font-semibold tracking-tight">Live Logs</h1>
        <p className="text-sm text-muted-foreground">
          Browse all proxied LLM requests — click a row for full detail
        </p>
      </div>
      <LogsTable />
    </div>
  );
}
