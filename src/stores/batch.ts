import { create } from "zustand";
import { api } from "@/services/api";
import type { Analysis, AppError, DownloadOptions, Platform } from "@/types/api";
import { dedupeUrls, detectPlatform, urlKey } from "@/lib/urls";
import { randomId } from "@/lib/utils";

export type BatchStatus = "pending" | "analyzing" | "ready" | "error" | "queued";

export interface BatchItem {
  id: string;
  url: string;
  platform: Platform | null;
  status: BatchStatus;
  analysis: Analysis | null;
  error: AppError | null;
  /** Per-item options; `null` uses the global batch options. */
  options: DownloadOptions | null;
  selected: boolean;
  /** For playlists: indices of selected entries. */
  entries: number[];
}

interface BatchStore {
  items: BatchItem[];
  globalOptions: DownloadOptions | null;
  setGlobalOptions: (options: DownloadOptions) => void;
  outputDir: string | null;
  setOutputDir: (dir: string | null) => void;
  addUrls: (urls: string[]) => { added: number; duplicates: number };
  remove: (ids: string[]) => void;
  update: (id: string, patch: Partial<BatchItem>) => void;
  setAllSelected: (selected: boolean) => void;
  clear: (filter: "all" | "queued" | "invalid") => void;
  analyzeAll: (concurrency?: number) => Promise<void>;
  analyzeOne: (id: string) => Promise<void>;
  analyzing: boolean;
}

export const MAX_BATCH_ITEMS = 2000;

export const useBatch = create<BatchStore>((set, get) => ({
  items: [],
  globalOptions: null,
  setGlobalOptions: (globalOptions) => set({ globalOptions }),
  outputDir: null,
  setOutputDir: (outputDir) => set({ outputDir }),
  analyzing: false,

  addUrls: (urls) => {
    const existing = new Set(get().items.map((i) => urlKey(i.url)));
    const { unique, duplicates: internalDuplicates } = dedupeUrls(urls);
    const fresh = unique.filter((u) => !existing.has(urlKey(u)));
    const room = Math.max(0, MAX_BATCH_ITEMS - get().items.length);
    const accepted = fresh.slice(0, room);
    const items: BatchItem[] = accepted.map((url) => ({
      id: randomId(),
      url,
      platform: detectPlatform(url),
      status: "pending",
      analysis: null,
      error: null,
      options: null,
      selected: true,
      entries: [],
    }));
    set((s) => ({ items: [...s.items, ...items] }));
    return { added: items.length, duplicates: internalDuplicates + (unique.length - fresh.length) };
  },

  remove: (ids) => set((s) => ({ items: s.items.filter((i) => !ids.includes(i.id)) })),
  update: (id, patch) => set((s) => ({ items: s.items.map((i) => (i.id === id ? { ...i, ...patch } : i)) })),
  setAllSelected: (selected) => set((s) => ({ items: s.items.map((i) => ({ ...i, selected: i.status === "ready" ? selected : i.selected })) })),
  clear: (filter) =>
    set((s) => ({
      items:
        filter === "all"
          ? []
          : s.items.filter((i) => (filter === "queued" ? i.status !== "queued" : i.status !== "error")),
    })),

  analyzeOne: async (id) => {
    const item = get().items.find((i) => i.id === id);
    if (!item) return;
    get().update(id, { status: "analyzing", error: null });
    try {
      const analysis = await api.analyzeUrl(item.url, `batch-${id}`);
      const entries = analysis.kind === "playlist" ? analysis.entries.filter((e) => e.available).map((e) => e.index) : [];
      get().update(id, { status: "ready", analysis, entries, selected: true });
    } catch (error) {
      get().update(id, { status: "error", error: error as AppError, selected: false });
    }
  },

  analyzeAll: async (concurrency = 3) => {
    if (get().analyzing) return;
    set({ analyzing: true });
    try {
      const queue = get().items.filter((i) => i.status === "pending" || i.status === "error").map((i) => i.id);
      const worker = async () => {
        while (queue.length > 0) {
          const id = queue.shift();
          if (id) await get().analyzeOne(id);
        }
      };
      await Promise.all(Array.from({ length: Math.min(concurrency, queue.length) }, worker));
    } finally {
      set({ analyzing: false });
    }
  },
}));
