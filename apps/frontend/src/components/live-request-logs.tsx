"use client";

import { useQuery } from "@tanstack/react-query";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Badge } from "@/components/ui/badge";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Activity, RefreshCw } from "lucide-react";
import { useAuth } from "@/lib/auth";
import { fetchLogs, type LogEntry } from "@/lib/api";

function formatTime(isoTimestamp: string): string {
  const date = new Date(isoTimestamp);
  return date.toLocaleTimeString("en-GB", { hour12: false });
}

export function LiveRequestLogs() {
  const { token } = useAuth();

  const { data: logs = [], isLoading, refetch } = useQuery<LogEntry[]>({
    queryKey: ["request-logs"],
    queryFn: () => fetchLogs(token!, { limit: 50 }),
    enabled: !!token,
    refetchInterval: 5000,
  });

  return (
    <Card className="flex flex-col">
      <CardHeader className="flex flex-row items-center justify-between pb-3">
        <div className="space-y-1">
          <CardTitle className="flex items-center gap-2 text-base">
            <Activity className="h-4 w-4 text-primary" />
            Live Request Logs
          </CardTitle>
          <CardDescription>Proxied LLM requests — PII masked before forwarding</CardDescription>
        </div>
        <Button
          size="sm"
          className="gap-1.5 bg-primary text-white hover:bg-primary/90"
          onClick={() => refetch()}
        >
          <RefreshCw className="h-3.5 w-3.5" />
          Refresh
        </Button>
      </CardHeader>
      <CardContent className="p-0">
        {/* Table header */}
        <div className="grid grid-cols-[80px_1fr_90px_70px_80px_70px] gap-3 border-b bg-muted/50 px-4 py-2 text-xs font-medium text-muted-foreground">
          <span>Time</span>
          <span>Masked Prompt</span>
          <span>Provider</span>
          <span>PII Found</span>
          <span>Route</span>
          <span className="text-right">Latency</span>
        </div>
        <ScrollArea className="h-[340px]">
          <div className="divide-y">
            {isLoading ? (
              <div className="px-4 py-8 text-center text-sm text-muted-foreground">
                Loading logs...
              </div>
            ) : logs.length === 0 ? (
              <div className="px-4 py-8 text-center text-sm text-muted-foreground">
                No requests yet. Send a request through the proxy to see it here.
              </div>
            ) : (
              logs.map((log) => (
                <div
                  key={log.id}
                  className="grid grid-cols-[80px_1fr_90px_70px_80px_70px] items-center gap-3 px-4 py-3 text-sm hover:bg-muted/30 transition-colors"
                >
                  <span className="font-mono text-xs text-muted-foreground">
                    {formatTime(log.timestamp)}
                  </span>
                  <span className="truncate font-mono text-xs text-foreground">
                    {log.masked_prompt || "—"}
                  </span>
                  <span className="capitalize text-xs text-muted-foreground">{log.provider}</span>
                  <span>
                    {log.pii_detected_count > 0 ? (
                      <Badge variant="warning">{log.pii_detected_count} PII</Badge>
                    ) : (
                      <Badge variant="outline">Clean</Badge>
                    )}
                  </span>
                  <span>
                    <Badge variant={log.route === "local" ? "secondary" : "success"}>
                      {log.route}
                    </Badge>
                  </span>
                  <span className="text-right font-mono text-xs text-muted-foreground">
                    {log.latency_ms}ms
                  </span>
                </div>
              ))
            )}
          </div>
        </ScrollArea>
      </CardContent>
    </Card>
  );
}
