"use client";

import Link from "next/link";
import { usePathname, useRouter } from "next/navigation";
import { Shield, Activity, Settings, BarChart3, Key, GitBranch, LogOut } from "lucide-react";
import { cn } from "@/lib/utils";
import { Separator } from "@/components/ui/separator";
import { useAuth } from "@/lib/auth";

const navItems = [
  { label: "Dashboard", href: "/", icon: BarChart3 },
  { label: "Live Logs", href: "/logs", icon: Activity },
  { label: "Routing Rules", href: "/routing", icon: GitBranch },
  { label: "API Keys", href: "/keys", icon: Key },
  { label: "Settings", href: "/settings", icon: Settings },
];

export function Sidebar() {
  const { userEmail, logout } = useAuth();
  const pathname = usePathname();
  const router = useRouter();

  function handleLogout() {
    logout();
    router.replace("/login");
  }

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
              pathname === href
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
      <div className="flex flex-col gap-3 px-5 py-4">
        <div className="flex items-center gap-2">
          <span className="inline-flex h-2 w-2 rounded-full bg-emerald-500" />
          <span className="text-xs text-muted-foreground">Proxy running · port 8080</span>
        </div>
        {userEmail && (
          <div className="flex items-center justify-between">
            <span className="truncate text-xs text-muted-foreground">{userEmail}</span>
            <button
              onClick={handleLogout}
              className="ml-2 shrink-0 text-muted-foreground transition-colors hover:text-foreground"
              aria-label="Log out"
            >
              <LogOut className="h-3.5 w-3.5" />
            </button>
          </div>
        )}
      </div>
    </aside>
  );
}
