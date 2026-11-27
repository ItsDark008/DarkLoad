import { beforeEach, describe, expect, it, vi } from "vitest";
import type { BrowserVideo } from "@/types/api";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(() => Promise.resolve()) }));

import { invoke } from "@tauri-apps/api/core";
import { MAX_BROWSER_SELECTION, useBrowser } from "./browser";

function video(id: string, patch: Partial<BrowserVideo> = {}): BrowserVideo {
  return {
    id,
    url: `https://www.youtube.com/watch?v=${id}`,
    title: null,
    uploader: null,
    duration: null,
    thumbnail: `https://i.ytimg.com/vi/${id}/mqdefault.jpg`,
    ...patch,
  };
}

const id = (n: number) => `video${String(n).padStart(6, "0")}`;

function syncCalls(): string[][] {
  return vi
    .mocked(invoke)
    .mock.calls.filter(([command]) => command === "browser_sync_selection")
    .map(([, args]) => (args as { ids: string[] }).ids);
}

describe("browser selection", () => {
  beforeEach(() => {
    useBrowser.setState({ selection: [], current: null, loading: false });
    vi.mocked(invoke).mockClear();
  });

  it("adds and removes videos ticked in the page", () => {
    const { apply } = useBrowser.getState();
    apply({ kind: "toggle", selected: true, video: video(id(1), { title: "First" }) });
    apply({ kind: "toggle", selected: true, video: video(id(2)) });
    apply({ kind: "toggle", selected: true, video: video(id(1)) });
    expect(useBrowser.getState().selection.map((v) => v.id)).toEqual([id(1), id(2)]);
    // A repeated report keeps details it does not carry.
    expect(useBrowser.getState().selection[0]?.title).toBe("First");
    apply({ kind: "toggle", selected: false, video: video(id(1)) });
    expect(useBrowser.getState().selection.map((v) => v.id)).toEqual([id(2)]);
  });

  it("tracks the open page and completes picked videos from it", () => {
    const { apply, add } = useBrowser.getState();
    add(video(id(1)));
    apply({ kind: "page", url: "https://www.youtube.com/watch?v=" + id(1), title: "Song", video: video(id(1), { title: "Song", uploader: "Band", duration: 200 }) });
    const state = useBrowser.getState();
    expect(state.url).toBe("https://www.youtube.com/watch?v=" + id(1));
    expect(state.current?.id).toBe(id(1));
    expect(state.selection[0]).toMatchObject({ title: "Song", uploader: "Band", duration: 200 });
    apply({ kind: "page", url: "https://www.youtube.com/", title: "YouTube", video: null });
    expect(useBrowser.getState().current).toBeNull();
    apply({ kind: "loading", loading: true });
    expect(useBrowser.getState().loading).toBe(true);
  });

  it("caps the selection", () => {
    const { add } = useBrowser.getState();
    for (let n = 0; n < MAX_BROWSER_SELECTION; n++) expect(add(video(id(n)))).toBe(true);
    expect(add(video(id(MAX_BROWSER_SELECTION)))).toBe(false);
    expect(useBrowser.getState().selection).toHaveLength(MAX_BROWSER_SELECTION);
  });

  it("mirrors selection changes back to the page once per change", () => {
    const { add, remove, clear } = useBrowser.getState();
    add(video(id(1)));
    add(video(id(2)));
    add(video(id(2), { title: "Same set" }));
    remove(id(1));
    clear();
    expect(syncCalls()).toEqual([[id(1)], [id(1), id(2)], [id(2)], []]);
  });
});
