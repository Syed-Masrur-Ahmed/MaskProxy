import { Card, CardContent } from "@/components/ui/card";
import { ArrowUpRight, ShieldCheck, Zap, Server } from "lucide-react";

const stats = [
  {
    label: "Requests Today",
    value: "1,284",
    delta: "+12%",
    icon: ArrowUpRight,
  },
  {
    label: "PII Entities Masked",
    value: "3,847",
    delta: "+8%",
    icon: ShieldCheck,
  },
  {
    label: "Avg Added Latency",
    value: "6.2ms",
    delta: "−0.4ms",
    icon: Zap,
  },
  {
    label: "Local Routes",
    value: "38%",
    delta: "+5%",
    icon: Server,
  },
];

export function StatsBar() {
  return (
    <div className="grid grid-cols-2 gap-4 lg:grid-cols-4">
      {stats.map(({ label, value, delta, icon: Icon }) => (
        <Card key={label}>
          <CardContent className="flex items-start justify-between p-4">
            <div className="space-y-1">
              <p className="text-xs text-muted-foreground">{label}</p>
              <p className="text-2xl font-semibold">{value}</p>
              <p className="text-xs text-emerald-600">{delta} vs yesterday</p>
            </div>
            <div className="flex h-9 w-9 items-center justify-center rounded-lg bg-primary/10">
              <Icon className="h-4 w-4 text-primary" />
            </div>
          </CardContent>
        </Card>
      ))}
    </div>
  );
}
