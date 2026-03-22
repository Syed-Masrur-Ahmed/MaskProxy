import Link from "next/link";
import { Shield, Activity, Settings, BarChart3, Key, GitBranch } from "lucide-react";
import { cn } from "@/lib/utils";
import { Separator } from "@/components/ui/separator";

const navItems = [
  { label: "Dashboard", href: "/", icon: BarChart3 },
  { label: "Live Logs", href: "/logs", icon: Activity },
  { label: "Routing Rules", href: "/routing", icon: GitBranch },
  { label: "API Keys", href: "/keys", icon: Key },
  { label: "Settings", href: "/settings", icon: Settings },
];

export function Sidebar({ activePath = "/" }: { activePath?: string }) {
  return (
    <aside className="flex h-screen w-60 flex-col border-r bg-sidebar">
      {/* Logo */}
      <div className="flex h-16 items-center gap-2.5 px-5">
        <div className="flex h-8 w-8 items-center justify-center rounded-lg bg-primary">
          <Shield className="h-4 w-4 text-primary-foreground" />
        </div>
        <div className="flex flex-col leading-none">
          <span className="text-sm font-semibold text-sidebar-foreground">MaskProxy</span>
          <span className="text-xs text-muted-foreground">Privacy Middleware</span>
        </div>
      </div>

      <Separator />

      {/* Nav */}
      <nav className="flex-1 space-y-1 px-3 py-4">
        {navItems.map(({ label, href, icon: Icon }) => (
          <Link
            key={href}
            href={href}
            className={cn(
              "flex items-center gap-3 rounded-md px-3 py-2 text-sm font-medium transition-colors",
              activePath === href
                ? "bg-sidebar-accent text-sidebar-accent-foreground"
                : "text-sidebar-foreground hover:bg-sidebar-accent hover:text-sidebar-accent-foreground"
            )}
          >
            <Icon className="h-4 w-4 shrink-0" />
            {label}
          </Link>
        ))}
      </nav>

      <Separator />

      {/* Footer */}
      <div className="px-5 py-4">
        <div className="flex items-center gap-2">
          <span className="inline-flex h-2 w-2 rounded-full bg-emerald-500" />
          <span className="text-xs text-muted-foreground">Proxy running · port 8080</span>
        </div>
      </div>
    </aside>
  );
}
