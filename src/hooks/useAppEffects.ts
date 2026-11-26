import { useEffect, useRef } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { toast } from "sonner";
import { readClipboardUrl } from "@/lib/clipboard";
import { api, events } from "@/services/api";
import { PAGE_ORDER, useApp } from "@/stores/app";
import { useHistorySignal } from "@/stores/signals";
import { useQueue } from "@/stores/queue";
import { lighten, readableTextOn } from "@/lib/utils";
import { urlKey } from "@/lib/urls";
import type { DependencyStatus, InstallProgress, Job, Settings } from "@/types/api";

/** Subscribes to backend events and loads the initial queue. */
export function useBackendEvents() {
  useEffect(() => {
    const unlisteners: Promise<() => void>[] = [
      listen<Job>(events.queueJob, (e) => useQueue.getState().upsertJob(e.payload)),
      listen<Job[]>(events.queueChanged, (e) => useQueue.getState().setJobs(e.payload)),
      listen<DependencyStatus[]>(events.depsChanged, (e) => useApp.getState().setDeps(e.payload)),
      listen<InstallProgress>(events.depsProgress, (e) => useApp.getState().setInstallProgress(e.payload)),
      listen<Settings>(events.settingsChanged, (e) => useApp.getState().setSettings(e.payload)),
      listen(events.historyChanged, () => useHistorySignal.getState().bump()),
    ];
    api
      .queueList()
      .then((jobs) => useQueue.getState().setJobs(jobs))
      .catch(() => useQueue.getState().setJobs([]));
    return () => {
      unlisteners.forEach((p) => void p.then((fn) => fn()).catch(() => undefined));
    };
  }, []);
}

/** Applies theme, accent, density and motion preferences to the document. */
export function useAppearance(settings: Settings | null) {
  useEffect(() => {
    if (!settings) return;
    const root = document.documentElement;
    root.classList.toggle("theme-oled", settings.theme === "oled");
    root.classList.toggle("theme-dark", settings.theme === "dark");
    root.classList.toggle("density-compact", settings.density === "compact");
    root.classList.toggle("no-motion", !settings.animations);
    root.style.setProperty("--dl-accent", settings.accentColor);
    root.style.setProperty("--dl-accent-hover", lighten(settings.accentColor, 0.28));
    root.style.setProperty("--dl-accent-fg", readableTextOn(settings.accentColor));
  }, [settings]);

  const native = settings?.nativeWindowFrame;
  useEffect(() => {
    if (native == null) return;
    void getCurrentWindow().setDecorations(native).catch(() => undefined);
  }, [native]);
}

/** Global keyboard shortcuts for navigation. */
export function useGlobalShortcuts() {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const target = e.target as HTMLElement | null;
      const typing = target && (target.tagName === "INPUT" || target.tagName === "TEXTAREA" || target.isContentEditable);
      const mod = e.ctrlKey || e.metaKey;
      const { navigate, setShortcutsOpen } = useApp.getState();
      if (mod && !e.shiftKey && !e.altKey && /^[1-7]$/.test(e.key)) {
        e.preventDefault();
        const page = PAGE_ORDER[Number(e.key) - 1];
        if (page) navigate(page);
      } else if (mod && e.key === ",") {
        e.preventDefault();
        navigate("settings");
      } else if (!typing && !mod && e.key === "?") {
        e.preventDefault();
        setShortcutsOpen(true);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);
}

/** Optional clipboard watcher: checks the clipboard when the window regains focus. */
export function useClipboardWatcher(enabled: boolean) {
  const seen = useRef<Set<string>>(new Set());
  useEffect(() => {
    if (!enabled) return;
    let unlisten: (() => void) | undefined;
    void getCurrentWindow()
      .onFocusChanged(({ payload: focused }) => {
        if (!focused) return;
        void readClipboardUrl().then((url) => {
          if (!url) return;
          const key = urlKey(url);
          if (seen.current.has(key)) return;
          seen.current.add(key);
          toast("Link detected in clipboard", {
            description: url,
            duration: 10_000,
            action: { label: "Analyze", onClick: () => useApp.getState().openInDownloader(url) },
          });
        });
      })
      .then((fn) => (unlisten = fn));
    return () => unlisten?.();
  }, [enabled]);
}

/** Asks for confirmation before the window closes while downloads are running. */
export function useCloseGuard(confirmEnabled: boolean, ask: () => Promise<boolean>) {
  const askRef = useRef(ask);
  useEffect(() => {
    askRef.current = ask;
  });
  useEffect(() => {
    const win = getCurrentWindow();
    let unlisten: (() => void) | undefined;
    void win
      .onCloseRequested(async (event) => {
        if (!confirmEnabled) return;
        const active = await api.hasActiveDownloads().catch(() => false);
        if (!active) return;
        event.preventDefault();
        if (await askRef.current()) await win.destroy();
      })
      .then((fn) => (unlisten = fn));
    return () => unlisten?.();
  }, [confirmEnabled]);
}

/** One automatic update check per launch, when enabled. */
export function useStartupUpdateCheck(enabled: boolean | undefined, configured: boolean | undefined) {
  const done = useRef(false);
  useEffect(() => {
    if (done.current || !enabled || !configured) return;
    done.current = true;
    api
      .checkAppUpdate()
      .then((u) => {
        useApp.getState().setUpdate(u);
        if (u.available) toast(`DarkLoad ${u.version} is available`, { action: { label: "View", onClick: () => useApp.getState().navigate("settings", "updates") } });
      })
      .catch(() => undefined);
  }, [enabled, configured]);
}
