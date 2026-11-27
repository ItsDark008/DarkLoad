import { useEffect, useRef } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { toast } from "sonner";
import { readClipboardUrl } from "@/lib/clipboard";
import { api, events } from "@/services/api";
import { PAGE_ORDER, useApp } from "@/stores/app";
import { useHistorySignal } from "@/stores/signals";
import { useQueue } from "@/stores/queue";
import { lighten, oledPalette, readableTextOn } from "@/lib/utils";
import { urlKey } from "@/lib/urls";
import type { DependencyStatus, InstallProgress, Job, Settings, UpdateInfo } from "@/types/api";

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
    // The OLED theme's surfaces follow the chosen base color; the Dark theme keeps its own.
    for (const [name, value] of Object.entries(oledPalette(settings.oledBackground))) {
      if (settings.theme === "oled") root.style.setProperty(name, value);
      else root.style.removeProperty(name);
    }
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
      if (mod && !e.shiftKey && !e.altKey && /^[1-8]$/.test(e.key)) {
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
/** Restarts DarkLoad into the downloaded update, reporting failures. */
export function installUpdateNow() {
  void api.installAppUpdate().catch((e: { message?: string }) => toast.error("Update failed", { description: e.message }));
}

/**
 * Follows the background updater: shows the last known status at startup and announces a new
 * version once per version, either as ready to install or as available to download.
 */
export function useUpdateNotifications(configured: boolean | undefined) {
  const announced = useRef<string | null>(null);
  useEffect(() => {
    if (!configured) return;
    const show = (u: UpdateInfo) => {
      useApp.getState().setUpdate(u);
      if (!u.available || !u.version) return;
      const key = `${u.version}:${u.ready}`;
      if (announced.current === key) return;
      const view = () => useApp.getState().navigate("settings", "updates");
      if (u.ready) {
        announced.current = key;
        toast(`DarkLoad ${u.version} is ready`, {
          description: "It installs when you close DarkLoad, or restart now.",
          duration: 15000,
          action: { label: "Restart now", onClick: installUpdateNow },
        });
      } else if (useApp.getState().settings?.updateMode !== "auto" || !u.installable) {
        announced.current = key;
        toast(`DarkLoad ${u.version} is available`, { action: { label: "View", onClick: view } });
      }
    };
    const unlisten = listen<UpdateInfo>(events.updaterStatus, (e) => show(e.payload));
    api
      .checkAppUpdate(false)
      .then(show)
      .catch(() => undefined);
    return () => void unlisten.then((fn) => fn()).catch(() => undefined);
  }, [configured]);
}
