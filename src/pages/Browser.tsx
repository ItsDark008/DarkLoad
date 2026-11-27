import { useEffect, useLayoutEffect, useRef, useState, type RefObject } from "react";
import { listen } from "@tauri-apps/api/event";
import { toast } from "sonner";
import { AlertCircle, ArrowDownToLine, ArrowLeft, ArrowRight, Check, Download, Home, Loader2, MousePointerClick, Plus, RotateCw, Settings2, Trash2, X } from "lucide-react";
import { DownloadOptionsForm } from "@/components/DownloadOptionsForm";
import { DownloadPresets } from "@/components/DownloadPresets";
import { EmptyState } from "@/components/common";
import { Thumb } from "@/components/Thumb";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Tooltip } from "@/components/ui/controls";
import { Dialog, DialogContent } from "@/components/ui/overlays";
import { formatDuration } from "@/lib/format";
import { AUDIO_FORMATS, CONTAINERS, QUALITY_MODES, optionsForPlatform, optionsFromSettings } from "@/lib/options";
import { api, events } from "@/services/api";
import { useApp } from "@/stores/app";
import { MAX_BROWSER_SELECTION, useBrowser } from "@/stores/browser";
import type { AppError, BrowserEvent, BrowserVideo, DownloadOptions, DownloadRequest } from "@/types/api";
import { OutputFolder } from "./Downloader";

function hostOf(url: string): string | null {
  try {
    return new URL(url).hostname;
  } catch {
    return null;
  }
}

function isPlaylist(url: string): boolean {
  try {
    const parsed = new URL(url);
    return parsed.searchParams.has("list") || parsed.pathname === "/playlist";
  } catch {
    return false;
  }
}

function onBlocked(url: string) {
  const host = hostOf(url);
  if (host === "accounts.google.com") {
    toast.message("Signing in isn't available here", {
      description: "DarkLoad's browser stays signed out. Public videos work without an account.",
    });
    return;
  }
  toast.message("This link leads outside YouTube", {
    description: host ?? url,
    action: { label: "Open in browser", onClick: () => void api.browserOpenBlocked().catch(() => undefined) },
  });
}

function useBrowserEvents() {
  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void listen<BrowserEvent>(events.browser, ({ payload }) => {
      if (payload.kind === "blocked") onBlocked(payload.url);
      else useBrowser.getState().apply(payload);
    })
      .then((fn) => {
        if (disposed) fn();
        else unlisten = fn;
      })
      .catch(() => undefined);
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);
}

/** A dialog drawn by this window would sit underneath the browser, so the browser steps aside. */
function overlayOpen(): boolean {
  return document.querySelector('[role="dialog"], [role="alertdialog"]') != null;
}

/**
 * Keeps the native browser view exactly over `target`. It is shown while this page is mounted and
 * hidden as soon as the page is left or a dialog opens.
 */
function useBrowserPlacement(target: RefObject<HTMLDivElement | null>, onError: (error: AppError | null) => void, attempt: number) {
  useLayoutEffect(() => {
    const el = target.current;
    if (!el) return;
    let frame = 0;
    let shown = false;
    let disposed = false;
    const place = () => {
      frame = 0;
      if (disposed) return;
      const rect = el.getBoundingClientRect();
      const bounds = { x: rect.left, y: rect.top, width: rect.width, height: rect.height };
      if (overlayOpen() || bounds.width < 2 || bounds.height < 2) {
        if (shown) {
          shown = false;
          void api.browserHide().catch(() => undefined);
        }
        return;
      }
      if (!shown) {
        shown = true;
        api
          .browserShow(bounds)
          .then(() => onError(null))
          .catch((error: AppError) => {
            shown = false;
            if (!disposed) onError(error);
          });
      } else {
        void api.browserSetBounds(bounds).catch(() => undefined);
      }
    };
    const schedule = () => {
      if (!frame) frame = requestAnimationFrame(place);
    };
    const resize = new ResizeObserver(schedule);
    resize.observe(el);
    // Dialogs are portalled into <body>.
    const dialogs = new MutationObserver(schedule);
    dialogs.observe(document.body, { childList: true });
    window.addEventListener("resize", schedule);
    schedule();
    return () => {
      disposed = true;
      cancelAnimationFrame(frame);
      resize.disconnect();
      dialogs.disconnect();
      window.removeEventListener("resize", schedule);
      void api.browserHide().catch(() => undefined);
    };
  }, [target, onError, attempt]);
}

function optionsSummary(options: DownloadOptions): string {
  if (options.mediaType === "audio") {
    const format = AUDIO_FORMATS.find((f) => f.value === options.audioFormat)?.label ?? options.audioFormat;
    return `Audio · ${format}`;
  }
  const quality = QUALITY_MODES.find((q) => q.value === options.qualityMode)?.label ?? options.qualityMode;
  const container = CONTAINERS.find((c) => c.value === options.container)?.label ?? options.container;
  return `Video · ${quality} · ${container}`;
}

function Toolbar() {
  const url = useBrowser((s) => s.url);
  const loading = useBrowser((s) => s.loading);
  const current = useBrowser((s) => s.current);
  const selected = useBrowser((s) => (s.current ? s.selection.some((v) => v.id === s.current?.id) : false));
  const add = useBrowser((s) => s.add);
  const remove = useBrowser((s) => s.remove);
  const openInDownloader = useApp((s) => s.openInDownloader);
  const [draft, setDraft] = useState<string | null>(null);
  const input = useRef<HTMLInputElement>(null);

  const run = (action: () => Promise<void>) => void action().catch((e: AppError) => toast.error("The browser could not do that", { description: e.message }));
  const playlist = isPlaylist(url);

  return (
    <div className="flex h-12 shrink-0 items-center gap-1 border-b border-line bg-sidebar px-3">
      <Tooltip content="Back">
        <Button size="icon-sm" variant="ghost" aria-label="Back" onClick={() => run(api.browserBack)}>
          <ArrowLeft />
        </Button>
      </Tooltip>
      <Tooltip content="Forward">
        <Button size="icon-sm" variant="ghost" aria-label="Forward" onClick={() => run(api.browserForward)}>
          <ArrowRight />
        </Button>
      </Tooltip>
      <Tooltip content="Reload">
        <Button size="icon-sm" variant="ghost" aria-label="Reload" onClick={() => run(api.browserReload)}>
          {loading ? <Loader2 className="animate-spin" /> : <RotateCw />}
        </Button>
      </Tooltip>
      <Tooltip content="YouTube home">
        <Button size="icon-sm" variant="ghost" aria-label="YouTube home" onClick={() => run(() => api.browserNavigate(""))}>
          <Home />
        </Button>
      </Tooltip>
      <form
        className="mx-1.5 min-w-0 flex-1"
        onSubmit={(e) => {
          e.preventDefault();
          const value = draft ?? url;
          run(() => api.browserNavigate(value));
          input.current?.blur();
        }}
      >
        <Input
          ref={input}
          className="h-8"
          value={draft ?? url}
          onFocus={(e) => {
            setDraft(url);
            e.currentTarget.select();
          }}
          onBlur={() => setDraft(null)}
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Escape") input.current?.blur();
          }}
          placeholder="Search YouTube or paste a YouTube link"
          aria-label="Search YouTube or enter a YouTube address"
          spellCheck={false}
        />
      </form>
      {current ? (
        <Button
          size="sm"
          variant={selected ? "secondary" : "primary"}
          onClick={() => {
            if (selected) remove(current.id);
            else if (!add(current)) toast.error(`Select at most ${MAX_BROWSER_SELECTION} videos at a time`);
          }}
        >
          {selected ? <Check /> : <Plus />} {selected ? "Selected" : "Select this video"}
        </Button>
      ) : null}
      {current || playlist ? (
        <Tooltip content={playlist ? "Analyze this playlist in the Downloader" : "Analyze this video in the Downloader"}>
          <Button size="sm" variant="ghost" onClick={() => openInDownloader(playlist || !current ? url : current.url)}>
            <ArrowDownToLine /> Open in Downloader
          </Button>
        </Tooltip>
      ) : null}
    </div>
  );
}

function SelectionRow({ video, onRemove }: { video: BrowserVideo; onRemove: () => void }) {
  const meta = [video.uploader, video.duration ? formatDuration(video.duration) : null].filter(Boolean).join(" · ");
  return (
    <li className="group flex items-center gap-3 px-4 py-2 hover:bg-card">
      <Thumb remote={video.thumbnail} className="aspect-video h-10 shrink-0" />
      <div className="min-w-0 flex-1">
        <div className="truncate text-[13px] text-fg" title={video.title ?? video.url}>
          {video.title ?? video.url}
        </div>
        {meta ? <div className="truncate text-xs text-fg-2">{meta}</div> : null}
      </div>
      <Button size="icon-sm" variant="ghost" aria-label={`Remove ${video.title ?? video.id}`} className="opacity-60 group-hover:opacity-100" onClick={onRemove}>
        <X />
      </Button>
    </li>
  );
}

function SelectionTray() {
  const settings = useApp((s) => s.settings);
  const navigate = useApp((s) => s.navigate);
  const { selection, remove, clear, options, setOptions, outputDir, setOutputDir } = useBrowser();
  const [editing, setEditing] = useState(false);
  const [submitting, setSubmitting] = useState(false);
  const effective = options ?? (settings ? optionsFromSettings(settings) : null);
  const folder = outputDir ?? settings?.downloadDir ?? "";

  const download = async () => {
    if (!effective || selection.length === 0) return;
    const chosen = optionsForPlatform(effective, "youtube");
    const requests: DownloadRequest[] = selection.map((v) => ({
      url: v.url,
      options: chosen,
      outputDir,
      title: v.title,
      uploader: v.uploader,
      thumbnail: v.thumbnail,
      duration: v.duration,
    }));
    setSubmitting(true);
    try {
      const jobs = await api.enqueue(requests);
      clear();
      toast.success(`${jobs.length} ${jobs.length === 1 ? "download" : "downloads"} added to the queue`, {
        action: { label: "View queue", onClick: () => navigate("queue") },
      });
    } catch (e) {
      toast.error("Could not add downloads", { description: (e as AppError).message });
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <aside aria-label="Selected videos" className="flex w-[340px] shrink-0 flex-col border-l border-line bg-sidebar">
      <div className="flex h-14 shrink-0 items-center justify-between gap-2 border-b border-line px-4">
        <div className="min-w-0">
          <div className="text-[13px] font-semibold text-fg">Selected videos</div>
          <div className="tabular text-xs text-fg-2">
            {selection.length === 0 ? "None yet" : `${selection.length} of ${MAX_BROWSER_SELECTION}`}
          </div>
        </div>
        <Button size="sm" variant="ghost" disabled={selection.length === 0} onClick={clear}>
          <Trash2 /> Clear
        </Button>
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto">
        {selection.length === 0 ? (
          <EmptyState
            icon={MousePointerClick}
            title="Pick videos as you browse"
            description="Hover a thumbnail and tick its box, or open a video and choose Select this video. Your picks gather here."
          />
        ) : (
          <ul className="py-1">
            {selection.map((video) => (
              <SelectionRow key={video.id} video={video} onRemove={() => remove(video.id)} />
            ))}
          </ul>
        )}
      </div>

      <div className="shrink-0 space-y-3 border-t border-line p-4">
        <div className="flex items-center justify-between gap-2">
          <div className="min-w-0">
            <div className="text-xs font-medium text-fg-2">Download as</div>
            <div className="truncate text-[13px] text-fg">{effective ? optionsSummary(effective) : "…"}</div>
          </div>
          <Button size="sm" disabled={!effective} onClick={() => setEditing(true)}>
            <Settings2 /> Settings
          </Button>
        </div>
        <div>
          <div className="mb-1.5 text-xs font-medium text-fg-2">Save to</div>
          <OutputFolder value={folder} onChange={setOutputDir} />
        </div>
        <Button variant="primary" size="lg" className="w-full" disabled={selection.length === 0 || submitting || !effective} onClick={() => void download()}>
          {submitting ? <Loader2 className="animate-spin" /> : <Download />}
          {selection.length === 0 ? "Download" : `Download ${selection.length} ${selection.length === 1 ? "video" : "videos"}`}
        </Button>
      </div>

      <Dialog open={editing} onOpenChange={setEditing}>
        {effective ? (
          <DialogContent
            title="Download settings"
            description="These settings apply to every selected video. Audio tracks are checked for each video."
            className="w-[min(760px,calc(100vw-48px))]"
            footer={
              <Button variant="primary" onClick={() => setEditing(false)}>
                Done
              </Button>
            }
          >
            <DownloadPresets page="browser" options={effective} outputDir={folder} onApply={(preset) => {
              setOptions(preset.options);
              setOutputDir(preset.outputDir);
            }} />
            <DownloadOptionsForm options={effective} onChange={(patch) => setOptions({ ...effective, ...patch })} />
          </DialogContent>
        ) : null}
      </Dialog>
    </aside>
  );
}

export function BrowserPage() {
  const viewport = useRef<HTMLDivElement>(null);
  const [error, setError] = useState<AppError | null>(null);
  const [attempt, setAttempt] = useState(0);
  useBrowserEvents();
  useBrowserPlacement(viewport, setError, attempt);

  return (
    <div className="flex h-full flex-col">
      <Toolbar />
      <div className="flex min-h-0 flex-1">
        <div ref={viewport} className="relative min-w-0 flex-1 bg-black">
          <div className="absolute inset-0 flex items-center justify-center p-6">
            {error ? (
              <div className="max-w-sm text-center">
                <AlertCircle className="mx-auto mb-3 size-6 text-danger" />
                <p className="text-[14px] font-medium text-fg">The browser could not open</p>
                <p className="mt-1 text-[13px] text-fg-2" data-selectable>
                  {error.message}
                </p>
                <Button className="mt-4" onClick={() => setAttempt((n) => n + 1)}>
                  Try again
                </Button>
              </div>
            ) : (
              <Loader2 className="size-5 animate-spin text-fg-2" aria-label="Opening YouTube" />
            )}
          </div>
        </div>
        <SelectionTray />
      </div>
    </div>
  );
}
