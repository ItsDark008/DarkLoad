import { useEffect, useState, type ReactNode } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  ArrowDownToLine,
  ChevronsLeft,
  ChevronsRight,
  History,
  Info,
  Layers,
  LayoutGrid,
  ListVideo,
  Minus,
  Settings as SettingsIcon,
  Square,
  Copy,
  Globe,
  X,
  CircleArrowUp,
  type LucideIcon,
} from "lucide-react";
import { LogoMark } from "@/components/brand";
import { Tooltip } from "@/components/ui/controls";
import { cn, modKey } from "@/lib/utils";
import { depsHealth, useApp, type Page } from "@/stores/app";
import { summarize, useQueue } from "@/stores/queue";
import { formatPercent } from "@/lib/format";
import { useT } from "@/i18n";

type NavItem = { page: Page; label: string; icon: LucideIcon; key: string };

function workspaceNav(t: (text: string) => string): NavItem[] {
  return [
    { page: "overview", label: t("Overview"), icon: LayoutGrid, key: "1" },
    { page: "downloader", label: t("Downloader"), icon: ArrowDownToLine, key: "2" },
    { page: "batch", label: t("Batch Download"), icon: Layers, key: "3" },
    { page: "browser", label: t("YouTube Browser"), icon: Globe, key: "4" },
    { page: "queue", label: t("Download Queue"), icon: ListVideo, key: "5" },
    { page: "history", label: t("History"), icon: History, key: "6" },
  ];
}

function appNav(t: (text: string) => string): NavItem[] {
  return [
    { page: "settings", label: t("Settings"), icon: SettingsIcon, key: "7" },
    { page: "about", label: t("About"), icon: Info, key: "8" },
  ];
}

function useMaximized(enabled: boolean) {
  const [maximized, setMaximized] = useState(false);
  useEffect(() => {
    if (!enabled) return;
    const win = getCurrentWindow();
    let unlisten: (() => void) | undefined;
    void win.isMaximized().then(setMaximized).catch(() => undefined);
    void win
      .onResized(() => {
        void win.isMaximized().then(setMaximized).catch(() => undefined);
      })
      .then((fn) => (unlisten = fn))
      .catch(() => undefined);
    return () => unlisten?.();
  }, [enabled]);
  return maximized;
}

async function requestClose() {
  await getCurrentWindow().close();
}

function TitleBar() {
  const t = useT();
  const maximized = useMaximized(true);
  const jobs = useQueue((s) => s.jobs);
  const summary = summarize(jobs);
  const win = getCurrentWindow();
  const control = "flex h-full w-11 items-center justify-center text-fg-2 transition-colors hover:bg-hover hover:text-fg focus-visible:outline-none focus-visible:bg-hover";
  return (
    <header data-tauri-drag-region className="relative z-40 flex h-9 shrink-0 items-center justify-between border-b border-line bg-sidebar select-none">
      <div data-tauri-drag-region className="flex items-center gap-2 pl-3.5 text-xs text-fg-2">
        <LogoMark className="pointer-events-none size-3.5 text-fg" />
        <span data-tauri-drag-region className="font-medium text-fg">
          DarkLoad
        </span>
        {summary.active > 0 ? (
          <span data-tauri-drag-region className="tabular text-fg-2">
            · {t("{count} active", { count: summary.running })}{summary.fraction != null ? ` · ${formatPercent(summary.fraction)}` : ""}
          </span>
        ) : null}
      </div>
      <div className="flex h-full">
        <button type="button" aria-label={t("Minimize")} className={control} onClick={() => void win.minimize()}>
          <Minus className="size-4" strokeWidth={1.5} />
        </button>
        <button type="button" aria-label={maximized ? t("Restore") : t("Maximize")} className={control} onClick={() => void win.toggleMaximize()}>
          {maximized ? <Copy className="size-3.5 -scale-x-100" strokeWidth={1.5} /> : <Square className="size-3.5" strokeWidth={1.5} />}
        </button>
        <button type="button" aria-label={t("Close")} className={cn(control, "hover:bg-danger hover:text-white")} onClick={() => void requestClose()}>
          <X className="size-4" strokeWidth={1.5} />
        </button>
      </div>
      {summary.active > 0 && summary.fraction != null ? (
        <div className="absolute inset-x-0 bottom-[-1px] h-px bg-line">
          <div className="h-px bg-accent transition-[width] duration-500" style={{ width: `${summary.fraction * 100}%` }} />
        </div>
      ) : null}
    </header>
  );
}

function Sidebar({ collapsed, onToggle }: { collapsed: boolean; onToggle: () => void }) {
  const t = useT();
  const page = useApp((s) => s.page);
  const navigate = useApp((s) => s.navigate);
  const deps = useApp((s) => s.deps);
  const depsLoaded = useApp((s) => s.depsLoaded);
  const info = useApp((s) => s.info);
  const update = useApp((s) => s.update);
  const jobs = useQueue((s) => s.jobs);
  const active = summarize(jobs).active;
  const health = depsLoaded ? depsHealth(deps) : "unknown";
  const healthLabel = {
    ok: t("Components ready"),
    warning: t("Component attention"),
    error: t("Setup required"),
    unknown: t("Checking components"),
  }[health];
  const healthColor = { ok: "bg-success", warning: "bg-warning", error: "bg-danger", unknown: "bg-muted" }[health];

  function renderNavItem(item: NavItem) {
    const Icon = item.icon;
    const selected = page === item.page;
    return (
      <li key={item.page}>
        <Tooltip content={item.label} side="right" shortcut={`${modKey}+${item.key}`} disabled={!collapsed}>
          <button
            type="button"
            onClick={() => navigate(item.page)}
            aria-current={selected ? "page" : undefined}
            aria-label={collapsed ? item.label : undefined}
            className={cn(
              "relative flex h-9 w-full items-center gap-3 rounded-md text-[13px] font-medium transition-colors outline-none focus-visible:ring-2 focus-visible:ring-accent-hover/70",
              collapsed ? "justify-center" : "px-2.5",
              selected ? "bg-elevated text-fg" : "text-fg-2 hover:bg-card hover:text-fg",
            )}
          >
            {selected ? <span className="absolute top-2 bottom-2 left-0 w-[2px] rounded-full bg-accent" aria-hidden /> : null}
            <Icon className={cn("size-[18px] shrink-0", selected && "text-accent-hover")} strokeWidth={1.8} />
            {!collapsed ? <span className="flex-1 truncate text-left">{item.label}</span> : null}
            {item.page === "queue" && active > 0 ? (
              <span
                className={cn(
                  "tabular rounded-full bg-accent px-1.5 text-[10px] leading-4 font-semibold text-accent-fg",
                  collapsed && "absolute top-1 right-1",
                )}
              >
                {active}
              </span>
            ) : null}
          </button>
        </Tooltip>
      </li>
    );
  }

  return (
    <nav
      aria-label={t("Main")}
      className={cn(
        "flex shrink-0 flex-col border-r border-line bg-sidebar transition-[width] duration-200 ease-out",
        collapsed ? "w-[60px]" : "w-[228px]",
      )}
    >
      <div className={cn("flex h-16 shrink-0 items-center gap-2.5", collapsed ? "justify-center" : "px-5")}>
        <LogoMark className="size-7 text-fg" />
        {!collapsed ? <span className="truncate text-[15px] font-semibold tracking-tight">DarkLoad</span> : null}
      </div>

      <div className={cn("flex min-h-0 flex-1 flex-col gap-3 overflow-y-auto py-1", collapsed ? "px-2" : "px-3")}>
        <ul aria-label={t("Download workspace")} className="flex shrink-0 flex-col gap-0.5">
          {workspaceNav(t).map(renderNavItem)}
        </ul>
        <ul aria-label={t("Application")} className="mt-auto flex shrink-0 flex-col gap-0.5 border-t border-line pt-2 pb-1">
          {appNav(t).map(renderNavItem)}
        </ul>
      </div>

      <div className={cn("flex shrink-0 flex-col gap-1 border-t border-line py-3", collapsed ? "items-center px-2" : "px-3")}>
        {update?.available ? (
          <Tooltip content={t("Version {version} is available", { version: update.version ?? "" })} side="right" disabled={!collapsed}>
            <button
              type="button"
              onClick={() => navigate("settings", "updates")}
              aria-label={collapsed ? t("Update {version} available", { version: update.version ?? "" }) : undefined}
              className={cn(
                "flex h-8 items-center gap-2.5 rounded-md text-xs font-medium text-accent-hover transition-colors hover:bg-card",
                collapsed ? "w-9 justify-center" : "w-full px-2.5",
              )}
            >
              <CircleArrowUp className="size-4 shrink-0" />
              {!collapsed ? <span className="truncate">{t("Update {version} available", { version: update.version ?? "" })}</span> : null}
            </button>
          </Tooltip>
        ) : null}
        <Tooltip content={healthLabel} side="right" disabled={!collapsed}>
          <button
            type="button"
            onClick={() => navigate("settings", "dependencies")}
            className={cn(
              "flex h-8 items-center gap-2.5 rounded-md text-xs text-fg-2 transition-colors hover:bg-card hover:text-fg",
              collapsed ? "w-9 justify-center" : "w-full px-2.5",
            )}
          >
            <span className={cn("size-2 shrink-0 rounded-full", healthColor)} aria-hidden />
            {!collapsed ? <span className="truncate">{healthLabel}</span> : <span className="sr-only">{healthLabel}</span>}
          </button>
        </Tooltip>
        <div className={cn("flex items-center", collapsed ? "flex-col gap-1" : "justify-between pl-2.5")}>
          {!collapsed ? <span className="tabular text-[11px] text-muted">v{info?.version ?? "…"}</span> : null}
          <Tooltip content={collapsed ? t("Expand sidebar") : t("Collapse sidebar")} side="right" shortcut={`${modKey}+B`}>
            <button
              type="button"
              onClick={onToggle}
              aria-label={collapsed ? t("Expand sidebar") : t("Collapse sidebar")}
              className="flex size-8 items-center justify-center rounded-md text-fg-2 transition-colors hover:bg-card hover:text-fg"
            >
              {collapsed ? <ChevronsRight className="size-4" /> : <ChevronsLeft className="size-4" />}
            </button>
          </Tooltip>
        </div>
      </div>
    </nav>
  );
}

function useNarrowWindow(threshold: number) {
  const [narrow, setNarrow] = useState(() => window.innerWidth < threshold);
  useEffect(() => {
    const onResize = () => setNarrow(window.innerWidth < threshold);
    window.addEventListener("resize", onResize);
    return () => window.removeEventListener("resize", onResize);
  }, [threshold]);
  return narrow;
}

export function AppLayout({ children }: { children: ReactNode }) {
  const settings = useApp((s) => s.settings);
  const updateSettings = useApp((s) => s.updateSettings);
  const narrow = useNarrowWindow(1100);
  const auto = settings?.sidebarBehavior === "auto";
  const collapsed = auto ? narrow : (settings?.sidebarCollapsed ?? false);
  const nativeFrame = settings?.nativeWindowFrame ?? false;

  const toggle = () => {
    if (!settings) return;
    void updateSettings({ sidebarCollapsed: !collapsed, sidebarBehavior: "remember" });
  };

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && !e.shiftKey && !e.altKey && e.key.toLowerCase() === "b") {
        e.preventDefault();
        toggle();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });

  return (
    <div className="flex h-full flex-col bg-bg text-fg">
      {!nativeFrame ? <TitleBar /> : null}
      <div className="flex min-h-0 flex-1">
        <Sidebar collapsed={collapsed} onToggle={toggle} />
        <main id="main" className="min-w-0 flex-1 overflow-hidden">
          {children}
        </main>
      </div>
    </div>
  );
}
