"use client";

import { useState } from "react";
import { useQuery, keepPreviousData } from "@tanstack/react-query";
import { Activity, ChevronLeft, ChevronRight, RefreshCw } from "lucide-react";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Badge } from "@/components/ui/badge";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogClose,
  DialogCloseProvider,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { useAuth } from "@/lib/auth";
import { fetchLogs, type LogEntry } from "@/lib/api";
import { MaskedPrompt } from "@/components/masked-prompt";

const PAGE_SIZE_OPTIONS = [25, 50, 100, 200] as const;
const COLUMNS =
  "grid-cols-[80px_1fr_90px_120px_70px_70px_70px_70px]";

function formatTime(iso: string): string {
  return new Date(iso).toLocaleTimeString("en-GB", { hour12: false });
}

function formatFullTime(iso: string): string {
  const d = new Date(iso);
  return `${d.toLocaleDateString()} ${d.toLocaleTimeString("en-GB", { hour12: false })}`;
}

function statusVariant(code: number): "success" | "warning" | "destructive" {
  if (code >= 500) return "destructive";
  if (code >= 400) return "warning";
  return "success";
}

export function LogsTable() {
  const { token } = useAuth();
  const [page, setPage] = useState(0);
  const [pageSize, setPageSize] = useState<number>(50);
  const [selected, setSelected] = useState<LogEntry | null>(null);

  const {
    data = [],
    isLoading,
    isFetching,
    refetch,
  } = useQuery<LogEntry[]>({
    queryKey: ["logs-page", page, pageSize],
    queryFn: () =>
      fetchLogs(token!, { limit: pageSize + 1, offset: page * pageSize }),
    enabled: !!token,
    refetchInterval: page === 0 ? 5000 : false,
    placeholderData: keepPreviousData,
  });

  const hasNext = data.length > pageSize;
  const rows = hasNext ? data.slice(0, pageSize) : data;

  function changePageSize(next: number) {
    setPageSize(next);
    setPage(0);
  }

  return (
    <>
      <Card className="flex flex-col">
        <CardHeader className="flex flex-row items-center justify-between pb-3">
          <div className="space-y-1">
            <CardTitle className="flex items-center gap-2 text-base">
              <Activity className="h-4 w-4 text-primary" />
              Request Logs
            </CardTitle>
            <CardDescription>
              {page === 0
                ? "Auto-refreshes every 5 seconds. Click a row for full detail."
                : "Paused while paginating. Return to page 1 to resume live updates."}
            </CardDescription>
          </div>
          <Button
            size="sm"
            className="gap-1.5 bg-primary text-white hover:bg-primary/90"
            onClick={() => refetch()}
            disabled={isFetching}
          >
            <RefreshCw className={`h-3.5 w-3.5 ${isFetching ? "animate-spin" : ""}`} />
            Refresh
          </Button>
        </CardHeader>

        <CardContent className="p-0">
          <div
            className={`grid ${COLUMNS} gap-3 border-b bg-muted/50 px-4 py-2 text-xs font-medium text-muted-foreground`}
          >
            <span>Time</span>
            <span>Masked Prompt</span>
            <span>Provider</span>
            <span>Model</span>
            <span>PII</span>
            <span>Route</span>
            <span>Status</span>
            <span className="text-right">Latency</span>
          </div>

          <ScrollArea className="h-[520px]">
            <div className="divide-y">
              {isLoading ? (
                <div className="px-4 py-8 text-center text-sm text-muted-foreground">
                  Loading logs...
                </div>
              ) : rows.length === 0 ? (
                <div className="px-4 py-8 text-center text-sm text-muted-foreground">
                  {page === 0
                    ? "No requests yet. Send a request through the proxy to see it here."
                    : "No more logs on this page."}
                </div>
              ) : (
                rows.map((log) => (
                  <button
                    key={log.id}
                    type="button"
                    onClick={() => setSelected(log)}
                    className={`grid ${COLUMNS} w-full items-center gap-3 px-4 py-3 text-left text-sm transition-colors hover:bg-muted/30 cursor-pointer`}
                  >
                    <span className="font-mono text-xs text-muted-foreground">
                      {formatTime(log.timestamp)}
                    </span>
                    <MaskedPrompt
                      text={log.masked_prompt}
                      className="truncate font-mono text-xs text-foreground"
                    />
                    <span className="capitalize text-xs text-muted-foreground">
                      {log.provider}
                    </span>
                    <span className="truncate text-xs text-muted-foreground">
                      {log.model || "—"}
                    </span>
                    <span>
                      {log.pii_detected_count > 0 ? (
                        <Badge variant="warning">{log.pii_detected_count}</Badge>
                      ) : (
                        <Badge variant="outline">0</Badge>
                      )}
                    </span>
                    <span>
                      <Badge variant={log.route === "local" ? "secondary" : "success"}>
                        {log.route}
                      </Badge>
                    </span>
                    <span>
                      <Badge variant={statusVariant(log.status_code)}>
                        {log.status_code}
                      </Badge>
                    </span>
                    <span className="text-right font-mono text-xs text-muted-foreground">
                      {log.latency_ms}ms
                    </span>
                  </button>
                ))
              )}
            </div>
          </ScrollArea>

          <div className="flex items-center justify-between border-t px-4 py-3 text-sm">
            <div className="flex items-center gap-2 text-xs text-muted-foreground">
              <label htmlFor="page-size">Rows per page</label>
              <select
                id="page-size"
                value={pageSize}
                onChange={(e) => changePageSize(Number(e.target.value))}
                className="h-8 rounded-md border border-input bg-background px-2 text-xs focus:outline-none focus:ring-2 focus:ring-ring"
              >
                {PAGE_SIZE_OPTIONS.map((n) => (
                  <option key={n} value={n}>
                    {n}
                  </option>
                ))}
              </select>
            </div>

            <div className="flex items-center gap-3">
              <span className="text-xs text-muted-foreground">
                Page {page + 1} · {rows.length} {rows.length === 1 ? "row" : "rows"}
              </span>
              <div className="flex items-center gap-1">
                <Button
                  size="sm"
                  variant="outline"
                  onClick={() => setPage((p) => Math.max(0, p - 1))}
                  disabled={page === 0 || isFetching}
                  className="gap-1"
                >
                  <ChevronLeft className="h-3.5 w-3.5" />
                  Prev
                </Button>
                <Button
                  size="sm"
                  variant="outline"
                  onClick={() => setPage((p) => p + 1)}
                  disabled={!hasNext || isFetching}
                  className="gap-1"
                >
                  Next
                  <ChevronRight className="h-3.5 w-3.5" />
                </Button>
              </div>
            </div>
          </div>
        </CardContent>
      </Card>

      <LogDetailDialog log={selected} onClose={() => setSelected(null)} />
    </>
  );
}

function LogDetailDialog({
  log,
  onClose,
}: {
  log: LogEntry | null;
  onClose: () => void;
}) {
  return (
    <Dialog open={log !== null} onOpenChange={(o) => !o && onClose()}>
      <DialogCloseProvider onClose={onClose}>
        <DialogContent className="max-w-2xl">
          <DialogClose />
          {log && (
            <>
              <DialogHeader>
                <DialogTitle>Request Detail</DialogTitle>
                <DialogDescription>
                  {formatFullTime(log.timestamp)}
                </DialogDescription>
              </DialogHeader>

              <dl className="grid grid-cols-[140px_1fr] gap-x-4 gap-y-3 text-sm">
                <dt className="text-muted-foreground">Session ID</dt>
                <dd className="font-mono text-xs break-all">{log.session_id}</dd>

                <dt className="text-muted-foreground">Provider</dt>
                <dd className="capitalize">{log.provider}</dd>

                <dt className="text-muted-foreground">Model</dt>
                <dd className="font-mono text-xs">{log.model || "—"}</dd>

                <dt className="text-muted-foreground">Route</dt>
                <dd>
                  <Badge variant={log.route === "local" ? "secondary" : "success"}>
                    {log.route}
                  </Badge>
                </dd>

                <dt className="text-muted-foreground">Status</dt>
                <dd>
                  <Badge variant={statusVariant(log.status_code)}>
                    {log.status_code}
                  </Badge>
                </dd>

                <dt className="text-muted-foreground">Latency</dt>
                <dd className="font-mono text-xs">{log.latency_ms} ms</dd>

                <dt className="text-muted-foreground">PII detected</dt>
                <dd className="flex flex-wrap items-center gap-1.5">
                  <span>{log.pii_detected_count}</span>
                  {log.pii_types.map((t) => (
                    <Badge key={t} variant="warning">
                      {t}
                    </Badge>
                  ))}
                </dd>

                <dt className="text-muted-foreground self-start pt-1">
                  Masked prompt
                </dt>
                <dd>
                  <pre className="max-h-60 overflow-auto whitespace-pre-wrap rounded-md border bg-muted/40 p-3 font-mono text-xs">
                    <MaskedPrompt text={log.masked_prompt} />
                  </pre>
                </dd>
              </dl>
            </>
          )}
        </DialogContent>
      </DialogCloseProvider>
    </Dialog>
  );
}
