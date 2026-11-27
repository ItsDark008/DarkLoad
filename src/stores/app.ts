import { create } from "zustand";
import { toast } from "sonner";
import { api } from "@/services/api";
import type { AppInfo, DependencyStatus, DepId, InstallProgress, Settings, UpdateInfo } from "@/types/api";

export type Page = "overview" | "downloader" | "batch" | "browser" | "queue" | "history" | "settings" | "about";

export const PAGE_ORDER: Page[] = ["overview", "downloader", "batch", "browser", "queue", "history", "settings", "about"];

export type SettingsSection = "general" | "downloads" | "appearance" | "dependencies" | "updates" | "privacy";

interface AppStore {
  page: Page;
  settingsSection: SettingsSection;
  navigate: (page: Page, section?: SettingsSection) => void;

  /** A link handed to the Downloader page from elsewhere (Overview, clipboard, history). */
  pendingUrl: string | null;
  openInDownloader: (url: string) => void;
  consumePendingUrl: () => string | null;

  info: AppInfo | null;
  settings: Settings | null;
  setSettings: (settings: Settings) => void;
  updateSettings: (patch: Partial<Settings>) => Promise<boolean>;

  deps: DependencyStatus[];
  depsLoaded: boolean;
  setDeps: (deps: DependencyStatus[]) => void;
  installProgress: Partial<Record<DepId, InstallProgress>>;
  setInstallProgress: (progress: InstallProgress) => void;
  installing: boolean;
  installDeps: (ids: DepId[], repair?: boolean) => Promise<void>;

  update: UpdateInfo | null;
  setUpdate: (update: UpdateInfo | null) => void;

  shortcutsOpen: boolean;
  setShortcutsOpen: (open: boolean) => void;

  bootstrap: () => Promise<void>;
}

export const useApp = create<AppStore>((set, get) => ({
  page: "overview",
  settingsSection: "general",
  navigate: (page, section) => set((s) => ({ page, settingsSection: section ?? s.settingsSection })),

  pendingUrl: null,
  openInDownloader: (url) => set({ pendingUrl: url, page: "downloader" }),
  consumePendingUrl: () => {
    const url = get().pendingUrl;
    if (url) set({ pendingUrl: null });
    return url;
  },

  info: null,
  settings: null,
  setSettings: (settings) => set({ settings }),
  updateSettings: async (patch) => {
    const current = get().settings;
    if (!current) return false;
    const next = { ...current, ...patch };
    set({ settings: next });
    try {
      const saved = await api.settingsUpdate(next);
      set({ settings: saved });
      return true;
    } catch (error) {
      set({ settings: current });
      const e = error as { message?: string };
      toast.error("Setting not saved", { description: e.message });
      return false;
    }
  },

  deps: [],
  depsLoaded: false,
  setDeps: (deps) => set({ deps, depsLoaded: true }),
  installProgress: {},
  setInstallProgress: (progress) => set((s) => ({ installProgress: { ...s.installProgress, [progress.id]: progress } })),
  installing: false,
  installDeps: async (ids, repair = false) => {
    if (get().installing) return;
    set({ installing: true, installProgress: {} });
    try {
      const deps = await api.depsInstall(ids, repair);
      set({ deps, depsLoaded: true });
      toast.success(repair ? "Components repaired" : "Components installed", {
        description: "All selected components were verified and installed.",
      });
    } catch (error) {
      const e = error as { message?: string };
      toast.error("Installation failed", { description: e.message });
    } finally {
      set({ installing: false });
    }
  },

  update: null,
  setUpdate: (update) => set({ update }),

  shortcutsOpen: false,
  setShortcutsOpen: (open) => set({ shortcutsOpen: open }),

  bootstrap: async () => {
    const [info, settings] = await Promise.all([api.appInfo(), api.settingsGet()]);
    set({ info, settings });
    api
      .depsStatus()
      .then((deps) => set({ deps, depsLoaded: true }))
      .catch(() => set({ depsLoaded: true }));
  },
}));

export function missingRequiredDeps(deps: DependencyStatus[]): DependencyStatus[] {
  return deps.filter((d) => d.required && !d.available);
}

export function depsHealth(deps: DependencyStatus[]): "ok" | "warning" | "error" | "unknown" {
  if (deps.length === 0) return "unknown";
  if (missingRequiredDeps(deps).length > 0) return "error";
  if (deps.some((d) => !d.available || d.updateAvailable)) return "warning";
  return "ok";
}
