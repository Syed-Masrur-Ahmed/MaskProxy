"use client";

import { useQuery } from "@tanstack/react-query";
import { Card, CardContent } from "@/components/ui/card";
import { ArrowUpRight, ShieldCheck, Zap, Server } from "lucide-react";
import { useAuth } from "@/lib/auth";
import { fetchStats } from "@/lib/api";

export function StatsBar() {
  const { token } = useAuth();

  const { data: stats } = useQuery({
    queryKey: ["dashboard-stats"],
    queryFn: () => fetchStats(token!),
    enabled: !!token,
    refetchInterval: 10000,
  });

  const cards = [
    {
      label: "Requests Today",
      value: stats ? stats.requests_today.toLocaleString() : "—",
      icon: ArrowUpRight,
    },
    {
      label: "PII Entities Masked",
      value: stats ? stats.pii_entities_masked.toLocaleString() : "—",
      icon: ShieldCheck,
    },
    {
      label: "Avg Added Latency",
      value: stats ? `${stats.avg_latency_ms}ms` : "—",
      icon: Zap,
    },
    {
      label: "Local Routes",
      value: stats ? `${stats.local_route_pct}%` : "—",
      icon: Server,
    },
  ];

  return (
    <div className="grid grid-cols-2 gap-4 lg:grid-cols-4">
      {cards.map(({ label, value, icon: Icon }) => (
        <Card key={label}>
          <CardContent className="flex items-start justify-between p-4">
            <div className="space-y-1">
              <p className="text-xs text-muted-foreground">{label}</p>
              <p className="text-2xl font-semibold">{value}</p>
            </div>
            <Icon className="h-5 w-5 text-primary" />
          </CardContent>
        </Card>
      ))}
    </div>
  );
}
