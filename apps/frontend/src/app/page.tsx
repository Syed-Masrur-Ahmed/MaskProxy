import { StatsBar } from "@/components/stats-bar";
import { LiveRequestLogs } from "@/components/live-request-logs";
import { PrivacySettings } from "@/components/privacy-settings";

export default function DashboardPage() {
  return (
    <div className="flex flex-col gap-6 p-6">
      {/* Header */}
      <div>
        <h1 className="text-xl font-semibold tracking-tight">Developer Dashboard</h1>
        <p className="text-sm text-muted-foreground">
          Monitor proxied LLM requests and configure privacy controls
        </p>
      </div>

      {/* Stats row */}
      <StatsBar />

      {/* Main content */}
      <div className="grid grid-cols-1 gap-6 xl:grid-cols-[1fr_320px]">
        {/* Live Request Logs — spans full width on small screens */}
        <LiveRequestLogs />

        {/* Privacy Settings panel */}
        <PrivacySettings />
      </div>
    </div>
  );
}
