import { StatsBar } from "@/components/stats-bar";
import { LiveRequestLogs } from "@/components/live-request-logs";

export default function DashboardPage() {
  return (
    <div className="flex flex-col gap-6 p-6">
      {/* Header */}
      <div>
        <h1 className="text-xl font-semibold tracking-tight">Developer Dashboard</h1>
        <p className="text-sm text-muted-foreground">
          Monitor proxied LLM requests in real time
        </p>
      </div>

      {/* Stats row */}
      <StatsBar />

      {/* Main content */}
      <LiveRequestLogs />
    </div>
  );
}
