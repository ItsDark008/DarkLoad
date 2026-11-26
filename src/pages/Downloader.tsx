import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { toast } from "sonner";
import { ArrowDownToLine, FolderOpen, Link2, ListVideo, Loader2, Search, Square } from "lucide-react";
import { DownloadOptionsForm } from "@/components/DownloadOptionsForm";
import { EmptyState, InlineAlert, PageHeader } from "@/components/common";
import { FormatDetails, MediaCard, PlanSummary, PlaylistPicker } from "@/components/media";
import { UrlField } from "@/components/UrlField";
import { readClipboardUrl } from "@/lib/clipboard";
import { Button } from "@/components/ui/button";
import { Card, Kbd, SectionTitle, Skeleton } from "@/components/ui/controls";
import { ConfirmDialog } from "@/components/ui/overlays";
import { PlatformIcon } from "@/components/brand";
import { formatRelative } from "@/lib/format";
import { errorHint, optionsForPlatform, optionsFromSettings } from "@/lib/options";
import { detectPlatform } from "@/lib/urls";
import { modKey } from "@/lib/utils";
import { api } from "@/services/api";
import { missingRequiredDeps, useApp } from "@/stores/app";
import { useDownloader } from "@/stores/downloader";
import type { DownloadOptions, DownloadRequest, HistoryEntry, MediaInfo } from "@/types/api";

export function OutputFolder({ value, onChange }: { value: string; onChange: (dir: string) => void }) {
  const choose = async () => {
    const picked = await openDialog({ directory: true, multiple: false, defaultPath: value || undefined, title: "Choose download folder" });
    if (typeof picked !== "string") return;
    try {
      onChange(await api.validateDirectory(picked));
    } catch (error) {
      toast.error("Folder not usable", { description: (error as { message?: string }).message });
    }
  };
  return (
    <div className="flex items-center gap-2">
      <div className="flex h-9 min-w-0 flex-1 items-center gap-2 rounded-md border border-line bg-card px-3 text-[13px]" title={value}>
        <FolderOpen className="size-4 shrink-0 text-fg-2" />
        <span className="truncate text-fg" data-selectable>
          {value || "No folder selected"}
        </span>
      </div>
      <Button onClick={() => void choose()}>Change</Button>
    </div>
  );
}

function requestFor(media: MediaInfo, options: DownloadOptions, outputDir: string | null): DownloadRequest {
  return {
    url: media.url,
    playlistItem: media.playlistItem,
    options,
    outputDir,
    title: media.title,
    uploader: media.uploader,
    thumbnail: media.thumbnail,
    thumbnailLocal: media.thumbnailLocal,
    duration: media.duration,
    spotify: media.spotify ?? null,
  };
}

export function DownloaderPage() {
  const settings = useApp((s) => s.settings);
  const deps = useApp((s) => s.deps);
  const depsLoaded = useApp((s) => s.depsLoaded);
  const navigate = useApp((s) => s.navigate);
  const consumePendingUrl = useApp((s) => s.consumePendingUrl);
  const pendingUrl = useApp((s) => s.pendingUrl);
  const store = useDownloader();
  const { url, setUrl, status, analysis, error, analyze, cancel, options: storedOptions, setOptions, patchOptions, plan, planError, setPlan, selected, setSelected, outputDir, setOutputDir } = store;
  const isSpotify = analysis?.platform === "spotify";
  const options = useMemo(() => (storedOptions ? optionsForPlatform(storedOptions, analysis?.platform) : null), [storedOptions, analysis?.platform]);
  const inputRef = useRef<HTMLInputElement>(null);
  const [duplicate, setDuplicate] = useState<HistoryEntry | null>(null);
  const [submitting, setSubmitting] = useState(false);

  useEffect(() => {
    if (!storedOptions && settings) setOptions(optionsFromSettings(settings));
  }, [storedOptions, settings, setOptions]);

  useEffect(() => {
    const pending = consumePendingUrl();
    if (pending) {
      setUrl(pending);
      void analyze(pending);
    }
  }, [pendingUrl, consumePendingUrl, setUrl, analyze]);

  const media = analysis?.kind === "media" ? analysis : null;
  const playlist = analysis?.kind === "playlist" ? analysis : null;

  // Recompute the selection plan whenever the analyzed media or options change.
  useEffect(() => {
    if (!media || !options) return;
    let stale = false;
    const timer = window.setTimeout(() => {
      api
        .planDownload(media.url, media.playlistItem, options)
        .then((p) => !stale && setPlan(p, null))
        .catch((e) => !stale && setPlan(null, e));
    }, 120);
    return () => {
      stale = true;
      window.clearTimeout(timer);
    };
  }, [media, options, setPlan]);

  const missing = missingRequiredDeps(deps);
  const ytdlpMissing = depsLoaded && missing.some((d) => d.id === "yt-dlp");
  const folder = outputDir ?? settings?.downloadDir ?? "";

  const enqueue = useCallback(
    async (requests: DownloadRequest[]) => {
      setSubmitting(true);
      try {
        const jobs = await api.enqueue(requests);
        toast.success(jobs.length === 1 ? "Added to the queue" : `${jobs.length} downloads added to the queue`, {
          description: jobs.length === 1 ? jobs[0]?.title : undefined,
          action: { label: "View queue", onClick: () => navigate("queue") },
        });
      } catch (e) {
        toast.error("Could not start the download", { description: (e as { message?: string }).message });
      } finally {
        setSubmitting(false);
      }
    },
    [navigate],
  );

  const canDownload =
    !!options &&
    !submitting &&
    !ytdlpMissing &&
    ((media && plan && !plan.containerConflict && !planError) || (playlist && selected.size > 0)) &&
    !(options.qualityMode === "custom" && !options.customFormat?.trim());

  const download = useCallback(async () => {
    if (!options || !canDownload) return;
    if (media) {
      const existing = await api.historyFindCompleted(media.url).catch(() => null);
      // Only warn when the same kind of file (video or audio) was already saved.
      if (existing && existing.mediaType === options.mediaType) {
        setDuplicate(existing);
        return;
      }
      await enqueue([requestFor(media, options, outputDir)]);
    } else if (playlist) {
      const requests = playlist.entries
        .filter((e) => selected.has(e.index) && e.available)
        .map<DownloadRequest>((e) =>
          e.media
            ? requestFor(e.media, options, outputDir)
            : { url: e.url, options, outputDir, title: e.title, uploader: e.uploader, thumbnail: e.thumbnail, duration: e.duration, spotify: e.spotify ?? null },
        );
      await enqueue(requests);
    }
  }, [options, canDownload, media, playlist, selected, outputDir, enqueue]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const mod = e.ctrlKey || e.metaKey;
      if (mod && e.key === "Enter") {
        e.preventDefault();
        void download();
      } else if (mod && e.key.toLowerCase() === "l") {
        e.preventDefault();
        inputRef.current?.focus();
        inputRef.current?.select();
      } else if (mod && e.shiftKey && e.key.toLowerCase() === "v") {
        e.preventDefault();
        void readClipboardUrl().then((u) => {
          if (u) {
            setUrl(u);
            void analyze(u);
          }
        });
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [download, setUrl, analyze]);

  return (
    <div className="mx-auto flex max-w-[1180px] flex-col px-8 py-7">
      <PageHeader title="Downloader" description="Analyze a link to see exactly what the source offers, then download at the quality you choose." />

      <UrlField
        ref={inputRef}
        value={url}
        onChange={setUrl}
        onSubmit={(u) => void analyze(u)}
        autoFocus
        action={
          status === "analyzing" ? (
            <Button variant="secondary" onClick={cancel}>
              <Square className="size-3.5" /> Stop
            </Button>
          ) : (
            <Button type="submit" variant="primary" disabled={!url.trim() || ytdlpMissing}>
              <Search /> Analyze
            </Button>
          )
        }
      />

      {ytdlpMissing ? (
        <InlineAlert
          className="mt-4"
          tone="warning"
          title="Components required"
          action={
            <Button size="sm" variant="primary" onClick={() => navigate("settings", "dependencies")}>
              Set up
            </Button>
          }
        >
          DarkLoad needs yt-dlp and FFmpeg to analyze and download media. They are installed from their official releases and verified.
        </InlineAlert>
      ) : null}

      <div className="mt-6 grid min-h-0 grid-cols-1 gap-6 lg:grid-cols-[minmax(0,1fr)_360px]">
        <div className="min-w-0 space-y-5">
          {status === "idle" && !analysis ? (
            <Card>
              <EmptyState
                icon={Link2}
                title="Paste a link to begin"
                description={
                  <>
                    Supports YouTube videos, Shorts, playlists and archived livestreams, public TikTok videos, public Instagram reels and posts, and Spotify songs, albums and playlists. Press{" "}
                    <Kbd>{modKey}</Kbd> <Kbd>Shift</Kbd> <Kbd>V</Kbd> to paste and analyze.
                  </>
                }
                action={
                  <div className="flex gap-4 text-fg-2">
                    <PlatformIcon platform="youtube" className="size-5" />
                    <PlatformIcon platform="tiktok" className="size-5" />
                    <PlatformIcon platform="instagram" className="size-5" />
                    <PlatformIcon platform="spotify" className="size-5" />
                  </div>
                }
              />
            </Card>
          ) : null}

          {status === "analyzing" ? (
            <Card className="p-5" aria-busy="true">
              <div className="flex gap-4">
                <Skeleton className="aspect-video w-[220px]" />
                <div className="flex-1 space-y-2.5 py-1">
                  <Skeleton className="h-3 w-24" />
                  <Skeleton className="h-5 w-4/5" />
                  <Skeleton className="h-3.5 w-1/3" />
                  <div className="flex gap-2 pt-2">
                    <Skeleton className="h-5 w-16" />
                    <Skeleton className="h-5 w-20" />
                  </div>
                </div>
              </div>
              <div className="mt-5 flex items-center gap-2 text-xs text-fg-2">
                <Loader2 className="size-3.5 animate-spin" />{" "}
                {detectPlatform(url) === "spotify" ? "Reading the song list from Spotify and finding the audio on YouTube…" : "Reading available formats from the source…"}
              </div>
            </Card>
          ) : null}

          {status === "error" && error ? (
            <InlineAlert
              tone="danger"
              title={error.message}
              action={
                <Button size="sm" variant="secondary" onClick={() => void analyze()}>
                  Try again
                </Button>
              }
            >
              {errorHint(error.code) ?? (error.detail ? <span className="font-mono text-xs break-all">{error.detail}</span> : null)}
            </InlineAlert>
          ) : null}

          {media ? (
            <>
              <Card className="p-5">
                <MediaCard media={media} />
                {media.playlistId ? (
                  <div className="mt-4 flex items-center justify-between gap-3 rounded-lg border border-line bg-elevated px-3 py-2 text-[13px]">
                    <span className="flex items-center gap-2 text-fg-2">
                      <ListVideo className="size-4" /> This video is part of a playlist.
                    </span>
                    <Button
                      size="sm"
                      variant="ghost"
                      onClick={() => {
                        const listUrl = `https://www.youtube.com/playlist?list=${media.playlistId}`;
                        setUrl(listUrl);
                        void analyze(listUrl);
                      }}
                    >
                      Open playlist
                    </Button>
                  </div>
                ) : null}
              </Card>
              <Card className="p-5">
                <SectionTitle>{isSpotify ? "Source formats on YouTube" : "Source formats"}</SectionTitle>
                <FormatDetails media={media} />
              </Card>
            </>
          ) : null}

          {playlist ? (
            <Card className="p-5">
              <div className="mb-4 flex items-start justify-between gap-4">
                <div className="min-w-0">
                  <div className="flex items-center gap-2 text-xs text-fg-2">
                    <PlatformIcon platform={playlist.platform} className="size-3.5" />
                    {playlist.platform === "instagram" ? "Post with multiple videos" : playlist.platform === "spotify" && playlist.url.includes("/album/") ? "Album" : "Playlist"} · {playlist.entries.length} {isSpotify ? "songs" : "items"}
                  </div>
                  <h2 className="mt-1 truncate text-[16px] font-semibold" data-selectable>
                    {playlist.title}
                  </h2>
                  {playlist.uploader ? <div className="text-[13px] text-fg-2">{playlist.uploader}</div> : null}
                </div>
              </div>
              {playlist.notice ? (
                <InlineAlert className="mb-4" tone="warning" title="Part of this list">
                  {playlist.notice}
                </InlineAlert>
              ) : null}
              <PlaylistPicker playlist={playlist} selected={selected} onSelectedChange={setSelected} />
              <p className="mt-3 text-xs text-fg-2">
                {isSpotify
                  ? "Each selected song is matched on YouTube by title, artist and duration when its download starts. The chosen source is shown in the queue."
                  : "Each selected item is analyzed when its download starts, so the chosen quality applies to the formats that item actually offers."}
              </p>
            </Card>
          ) : null}
        </div>

        <aside className="min-w-0 space-y-4">
          <Card className="p-5">
            <SectionTitle>Download options</SectionTitle>
            {options ? (
              <DownloadOptionsForm options={options} onChange={patchOptions} summary={media?.summary ?? null} audioOnly={isSpotify} />
            ) : (
              <Skeleton className="h-40 w-full" />
            )}
          </Card>
          <Card className="space-y-4 p-5">
            <div>
              <div className="mb-1.5 text-xs font-medium text-fg-2">Save to</div>
              <OutputFolder value={folder} onChange={setOutputDir} />
            </div>
            {media && plan && options ? <PlanSummary plan={plan} options={options} onChange={patchOptions} /> : null}
            {media && planError ? (
              <InlineAlert tone="danger" title={planError.message}>
                {errorHint(planError.code)}
              </InlineAlert>
            ) : null}
            <Button variant="primary" size="lg" className="w-full" disabled={!canDownload} onClick={() => void download()}>
              {submitting ? <Loader2 className="animate-spin" /> : <ArrowDownToLine />}
              {playlist ? `Download ${selected.size} ${selected.size === 1 ? "item" : "items"}` : "Download"}
              <span className="ml-auto hidden text-xs font-normal opacity-70 xl:inline">{modKey}+Enter</span>
            </Button>
          </Card>
        </aside>
      </div>

      <ConfirmDialog
        open={duplicate != null}
        onOpenChange={(o) => !o && setDuplicate(null)}
        title="Already downloaded"
        description={
          duplicate ? (
            <>
              You downloaded this {formatRelative(duplicate.createdAt)} and the file still exists. Download it again? Your duplicate setting decides whether the new file is renamed, skipped or overwrites the old one.
            </>
          ) : null
        }
        confirmLabel="Download again"
        onConfirm={() => {
          setDuplicate(null);
          if (media && options) void enqueue([requestFor(media, options, outputDir)]);
        }}
      />
    </div>
  );
}
