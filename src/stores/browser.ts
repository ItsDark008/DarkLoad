import { create } from "zustand";
import { api } from "@/services/api";
import type { BrowserEvent, BrowserVideo, DownloadOptions } from "@/types/api";

/** Matches the backend's limit for one selection. */
export const MAX_BROWSER_SELECTION = 500;

interface BrowserStore {
  url: string;
  title: string | null;
  /** The video open in the browser, when the page is a watch or Shorts page. */
  current: BrowserVideo | null;
  loading: boolean;
  /** Picked videos, oldest first. */
  selection: BrowserVideo[];
  options: DownloadOptions | null;
  setOptions: (options: DownloadOptions) => void;
  outputDir: string | null;
  setOutputDir: (dir: string | null) => void;

  /** Applies an event reported by the browser page. */
  apply: (event: BrowserEvent) => void;
  add: (video: BrowserVideo) => boolean;
  remove: (id: string) => void;
  clear: () => void;
}

/** Keeps known details when a later report of the same video is missing some of them. */
function merge(existing: BrowserVideo, next: BrowserVideo): BrowserVideo {
  return {
    ...existing,
    title: next.title ?? existing.title,
    uploader: next.uploader ?? existing.uploader,
    duration: next.duration ?? existing.duration,
  };
}

export const useBrowser = create<BrowserStore>((set, get) => ({
  url: "https://www.youtube.com/",
  title: null,
  current: null,
  loading: false,
  selection: [],
  options: null,
  setOptions: (options) => set({ options }),
  outputDir: null,
  setOutputDir: (outputDir) => set({ outputDir }),

  apply: (event) => {
    switch (event.kind) {
      case "toggle":
        if (event.selected) get().add(event.video);
        else get().remove(event.video.id);
        break;
      case "page":
        set((s) => ({
          url: event.url,
          title: event.title,
          current: event.video,
          // Details from the watch page complete what the thumbnail offered.
          selection: event.video ? s.selection.map((v) => (v.id === event.video?.id ? merge(v, event.video) : v)) : s.selection,
        }));
        break;
      case "loading":
        set({ loading: event.loading });
        break;
      case "blocked":
        break;
    }
  },

  add: (video) => {
    const { selection } = get();
    const index = selection.findIndex((v) => v.id === video.id);
    if (index >= 0) {
      set({ selection: selection.map((v, i) => (i === index ? merge(v, video) : v)) });
      return true;
    }
    if (selection.length >= MAX_BROWSER_SELECTION) return false;
    set({ selection: [...selection, video] });
    return true;
  },
  remove: (id) => set((s) => ({ selection: s.selection.filter((v) => v.id !== id) })),
  clear: () => set({ selection: [] }),
}));

// The page shows its own ticks, so every change to the selection is mirrored back to it. When the
// page reported the change itself, the sync leaves the page as it is.
let synced = "";
useBrowser.subscribe((state) => {
  const ids = state.selection.map((v) => v.id);
  const key = ids.join(",");
  if (key === synced) return;
  synced = key;
  void api.browserSyncSelection(ids).catch(() => undefined);
});
