import { create } from "zustand";
import { api } from "@/services/api";
import type { Analysis, AppError, DownloadOptions, SelectionPlan } from "@/types/api";
import { randomId } from "@/lib/utils";
import { useApp } from "./app";

type Status = "idle" | "analyzing" | "ready" | "error";

interface DownloaderStore {
  url: string;
  setUrl: (url: string) => void;
  status: Status;
  analysis: Analysis | null;
  error: AppError | null;
  requestId: string | null;
  /** Analyzes the URL. Any analysis in flight is cancelled first. */
  analyze: (url?: string) => Promise<Analysis | null>;
  cancel: () => void;
  reset: () => void;

  options: DownloadOptions | null;
  setOptions: (options: DownloadOptions) => void;
  patchOptions: (patch: Partial<DownloadOptions>) => void;

  plan: SelectionPlan | null;
  planError: AppError | null;
  setPlan: (plan: SelectionPlan | null, error: AppError | null) => void;

  /** Indices of selected playlist entries. */
  selected: Set<number>;
  setSelected: (selected: Set<number>) => void;

  outputDir: string | null;
  setOutputDir: (dir: string | null) => void;
}

export const useDownloader = create<DownloaderStore>((set, get) => ({
  url: "",
  setUrl: (url) => set({ url }),
  status: "idle",
  analysis: null,
  error: null,
  requestId: null,

  analyze: async (input) => {
    const url = (input ?? get().url).trim();
    if (!url) return null;
    get().cancel();
    const requestId = randomId();
    set({ url, status: "analyzing", error: null, analysis: null, plan: null, planError: null, requestId, selected: new Set() });
    try {
      const analysis = await api.analyzeUrl(url, requestId);
      if (get().requestId !== requestId) return null;
      const selectAll = useApp.getState().settings?.playlistBehavior !== "select";
      const selected =
        analysis.kind === "playlist" && selectAll ? new Set(analysis.entries.filter((e) => e.available).map((e) => e.index)) : new Set<number>();
      set({ status: "ready", analysis, requestId: null, selected });
      return analysis;
    } catch (error) {
      if (get().requestId !== requestId) return null;
      const e = error as AppError;
      set({ status: e.code === "cancelled" ? "idle" : "error", error: e.code === "cancelled" ? null : e, requestId: null });
      return null;
    }
  },
  cancel: () => {
    const { requestId } = get();
    if (requestId) {
      void api.cancelAnalysis(requestId).catch(() => undefined);
      set({ requestId: null, status: "idle" });
    }
  },
  reset: () => {
    get().cancel();
    set({ url: "", status: "idle", analysis: null, error: null, plan: null, planError: null, selected: new Set() });
  },

  options: null,
  setOptions: (options) => set({ options }),
  patchOptions: (patch) => set((s) => (s.options ? { options: { ...s.options, ...patch } } : {})),

  plan: null,
  planError: null,
  setPlan: (plan, planError) => set({ plan, planError }),

  selected: new Set(),
  setSelected: (selected) => set({ selected }),

  outputDir: null,
  setOutputDir: (outputDir) => set({ outputDir }),
}));
