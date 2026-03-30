"use client";

import { useState } from "react";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Badge } from "@/components/ui/badge";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Activity, RefreshCw } from "lucide-react";

type LogEntry = {
  id: string;
  timestamp: string;
  provider: "openai" | "anthropic";
  model: string;
  piiDetected: number;
  route: "cloud" | "local";
  latencyMs: number;
  maskedPrompt: string;
};

const MOCK_LOGS: LogEntry[] = [
  {
    id: "req_01",
    timestamp: "14:32:01",
    provider: "openai",
    model: "gpt-4o",
    piiDetected: 3,
    route: "cloud",
    latencyMs: 6,
    maskedPrompt: "Schedule a meeting for [PERSON_1] at [EMAIL_1] on [DATE_1]",
  },
  {
    id: "req_02",
    timestamp: "14:31:48",
    provider: "openai",
    model: "gpt-4o-mini",
    piiDetected: 0,
    route: "cloud",
    latencyMs: 4,
    maskedPrompt: "Explain the concept of zero-knowledge proofs.",
  },
  {
    id: "req_03",
    timestamp: "14:31:22",
    provider: "anthropic",
    model: "claude-3-5-sonnet",
    piiDetected: 5,
    route: "local",
    latencyMs: 8,
    maskedPrompt: "Patient [PERSON_1] DOB [DATE_1] SSN [SSN_1] presents with…",
  },
  {
    id: "req_04",
    timestamp: "14:30:55",
    provider: "openai",
    model: "gpt-4o",
    piiDetected: 1,
    route: "cloud",
    latencyMs: 5,
    maskedPrompt: "Draft a reply to [PERSON_1] about the contract renewal.",
  },
  {
    id: "req_05",
    timestamp: "14:30:10",
    provider: "anthropic",
    model: "claude-3-haiku",
    piiDetected: 2,
    route: "local",
    latencyMs: 7,
    maskedPrompt: "Summarize the record for [PERSON_1] at [ADDRESS_1].",
  },
];

export function LiveRequestLogs() {
  const [logs] = useState<LogEntry[]>(MOCK_LOGS);

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
        <Button size="sm" className="gap-1.5 bg-primary text-white hover:bg-primary/90">
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
            {logs.map((log) => (
              <div
                key={log.id}
                className="grid grid-cols-[80px_1fr_90px_70px_80px_70px] items-center gap-3 px-4 py-3 text-sm hover:bg-muted/30 transition-colors"
              >
                <span className="font-mono text-xs text-muted-foreground">{log.timestamp}</span>
                <span className="truncate font-mono text-xs text-foreground">{log.maskedPrompt}</span>
                <span className="capitalize text-xs text-muted-foreground">{log.provider}</span>
                <span>
                  {log.piiDetected > 0 ? (
                    <Badge variant="warning">{log.piiDetected} PII</Badge>
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
                  {log.latencyMs}ms
                </span>
              </div>
            ))}
          </div>
        </ScrollArea>
      </CardContent>
    </Card>
  );
}
