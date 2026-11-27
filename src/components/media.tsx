import { useMemo, useRef, useState } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { AlertTriangle, Captions, Clock, Info, ListTree, Radio, Table2 } from "lucide-react";
import { PlatformIcon } from "@/components/brand";
import { Thumb } from "@/components/Thumb";
import { Button } from "@/components/ui/button";
import { Badge, Checkbox } from "@/components/ui/controls";
import { Dialog, DialogContent } from "@/components/ui/overlays";
import { codecLabel, formatBitrate, formatBytes, formatDuration } from "@/lib/format";
import { PLATFORM_NAMES } from "@/lib/urls";
import { cn } from "@/lib/utils";
import { tr, useT } from "@/i18n";
import type { Container, DownloadOptions, MediaFormat, MediaInfo, PlaylistEntry, PlaylistInfo, SelectionPlan } from "@/types/api";

export function MediaCard({ media }: { media: MediaInfo }) {
  const t = useT();
  const s = media.summary;
  const song = media.spotify ?? null;
  return (
    <div className="flex gap-4">
      <Thumb local={media.thumbnailLocal} remote={media.thumbnail} audio={!!song} className={cn("shrink-0", song ? "aspect-square w-[124px]" : "aspect-video w-[220px]")} alt="" />
      <div className="min-w-0 flex-1 py-0.5">
        <div className="flex items-center gap-2 text-xs text-fg-2">
          <PlatformIcon platform={media.platform} className="size-3.5" />
          {PLATFORM_NAMES[media.platform]}
          {media.wasLive ? (
            <Badge>
              <Radio /> {t("Archived livestream")}
            </Badge>
          ) : null}
        </div>
        <h2 className="mt-1.5 line-clamp-2 text-[16px] leading-snug font-semibold text-fg" data-selectable title={media.title}>
          {media.title}
        </h2>
        <div className="mt-1 truncate text-[13px] text-fg-2" data-selectable>
          {media.uploader ?? t("Unknown creator")}
          {song?.album ? <span className="text-muted"> · {song.album}</span> : null}
        </div>
        {song?.sourceUrl ? (
          <div className="mt-1 truncate text-xs text-muted" data-selectable title={song.sourceUrl}>
            {t("Audio source on YouTube:")} {song.sourceTitle ?? song.sourceUrl}
            {song.sourceChannel ? ` — ${song.sourceChannel}` : ""}
          </div>
        ) : null}
        <div className="mt-3 flex flex-wrap gap-1.5">
          {media.duration != null ? (
            <Badge>
              <Clock /> {formatDuration(media.duration)}
            </Badge>
          ) : null}
          {s.maxHeight && !song ? <Badge tone="accent">{t("Up to {quality}", { quality: `${s.maxHeight}p${s.maxFps && s.maxFps > 30 ? s.maxFps : ""}` })}</Badge> : null}
          {song?.releaseDate ? <Badge>{song.releaseDate.slice(0, 4)}</Badge> : null}
          {song?.explicit ? <Badge>{t("Explicit")}</Badge> : null}
          {s.hasHdr ? <Badge tone="accent">HDR</Badge> : null}
          {media.chapters > 0 ? (
            <Badge>
              <ListTree /> {media.chapters === 1 ? t("1 chapter") : t("{count} chapters", { count: media.chapters })}
            </Badge>
          ) : null}
          {media.subtitles.some((track) => !track.automatic) && !song ? (
            <Badge>
              <Captions /> {t("{count} subtitle tracks", { count: media.subtitles.filter((track) => !track.automatic).length })}
            </Badge>
          ) : null}
        </div>
      </div>
    </div>
  );
}

export function FormatDetails({ media }: { media: MediaInfo }) {
  const t = useT();
  const [open, setOpen] = useState(false);
  const s = media.summary;
  return (
    <div className="space-y-3">
      {s.resolutions.length > 0 ? (
        <div>
          <div className="mb-1.5 text-xs font-medium text-fg-2">{t("Available resolutions")}</div>
          <div className="overflow-hidden rounded-lg border border-line">
            <table className="w-full text-left text-[12px]">
              <thead className="bg-elevated text-fg-2">
                <tr>
                  <th className="px-3 py-1.5 font-medium">{t("Resolution")}</th>
                  <th className="px-3 py-1.5 font-medium">{t("Frame rates")}</th>
                  <th className="px-3 py-1.5 font-medium">{t("Codecs")}</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-line">
                {s.resolutions.map((r) => (
                  <tr key={r.height}>
                    <td className="px-3 py-1.5 text-fg">
                      {r.label}
                      {r.hdr ? <Badge tone="accent" className="ml-2">HDR</Badge> : null}
                    </td>
                    <td className="tabular px-3 py-1.5 text-fg-2">{r.frameRates.length ? r.frameRates.map((f) => `${f}`).join(" · ") : "—"}</td>
                    <td className="px-3 py-1.5 text-fg-2">{r.codecs.map(codecLabel).join(", ")}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>
      ) : null}
      <div className="flex flex-wrap items-center justify-between gap-2 text-xs text-fg-2">
        <span>
          {t("Audio:")} {s.audioCodecs.length ? s.audioCodecs.map(codecLabel).join(", ") : t("none")}
          {s.audioBitrates.length ? ` · ${s.audioBitrates.slice(0, 5).map((b) => `${b} kbps`).join(", ")}` : ""}
        </span>
        <Button size="sm" variant="ghost" onClick={() => setOpen(true)}>
          <Table2 /> {t("All {count} formats", { count: media.formats.length })}
        </Button>
      </div>
      <Dialog open={open} onOpenChange={setOpen}>
        <DialogContent title={t("Available formats")} description={t("Every stream the source offers, as reported by the extractor.")} className="w-[min(920px,calc(100vw-48px))]">
          <AllFormatsTable formats={media.formats} />
        </DialogContent>
      </Dialog>
    </div>
  );
}

function AllFormatsTable({ formats }: { formats: MediaFormat[] }) {
  const t = useT();
  const sorted = useMemo(
    () =>
      formats
        .filter((f) => f.ext !== "mhtml")
        .slice()
        .sort((a, b) => (b.height ?? 0) - (a.height ?? 0) || (b.tbr ?? b.abr ?? 0) - (a.tbr ?? a.abr ?? 0)),
    [formats],
  );
  return (
    <table className="w-full text-left text-[12px]" data-selectable>
      <thead className="sticky top-0 bg-card text-fg-2">
        <tr>
          {["ID", "Ext", t("Resolution"), "FPS", t("Range"), t("Video"), t("Audio"), t("Bitrate"), t("Size")].map((h) => (
            <th key={h} className="border-b border-line px-2 py-1.5 font-medium">
              {h}
            </th>
          ))}
        </tr>
      </thead>
      <tbody className="tabular divide-y divide-line">
        {sorted.map((f) => {
          const size = f.filesize ?? f.filesizeApprox;
          return (
            <tr key={f.id} className="text-fg-2">
              <td className="px-2 py-1 font-mono text-fg">{f.id}</td>
              <td className="px-2 py-1">{f.ext}</td>
              <td className="px-2 py-1">{f.width && f.height ? `${f.width}×${f.height}` : f.vcodec ? "—" : t("audio only")}</td>
              <td className="px-2 py-1">{f.fps ? Math.round(f.fps) : "—"}</td>
              <td className="px-2 py-1">{f.vcodec ? (f.dynamicRange ?? "SDR") : "—"}</td>
              <td className="max-w-[140px] truncate px-2 py-1">{f.vcodec ?? "—"}</td>
              <td className="max-w-[120px] truncate px-2 py-1">{f.acodec ?? "—"}</td>
              <td className="px-2 py-1">{formatBitrate(f.tbr ?? f.abr)}</td>
              <td className="px-2 py-1">
                {size ? `${f.filesize ? "" : "≈ "}${formatBytes(size)}` : "—"}
              </td>
            </tr>
          );
        })}
      </tbody>
    </table>
  );
}

export function PlanSummary({
  plan,
  options,
  onChange,
}: {
  plan: SelectionPlan;
  options: DownloadOptions;
  onChange: (patch: Partial<DownloadOptions>) => void;
}) {
  const t = useT();
  const conflict = plan.containerConflict;
  const containerLabel = (c: Container | null) => (c ? c.toUpperCase().replace("WEBM", "WebM") : null);
  return (
    <div className="space-y-2.5">
      <div className="flex items-start justify-between gap-4">
        <div className="min-w-0">
          <div className="text-xs text-fg-2">{t("Selected streams")}</div>
          <div className="mt-0.5 text-[13px] font-medium text-fg" data-selectable>
            {tr(plan.qualityLabel) || t("Best available")}
          </div>
        </div>
        <div className="shrink-0 text-right">
          <div className="text-xs text-fg-2">{t("Estimated size")}</div>
          <div className="tabular mt-0.5 text-[13px] font-medium text-fg">
            {plan.estimatedSize ? `${plan.sizeIsEstimate ? "≈ " : ""}${formatBytes(plan.estimatedSize)}` : t("Not reported")}
          </div>
        </div>
      </div>
      <div className="flex flex-wrap gap-1.5">
        {options.mediaType === "video" && plan.outputContainer && !conflict ? <Badge>{containerLabel(plan.outputContainer)}</Badge> : null}
        {plan.requiresMerge ? <Badge>{t("Video + audio merged, no re-encoding")}</Badge> : null}
        {options.mediaType === "audio" ? <Badge tone={plan.transcodesAudio ? "warning" : "success"}>{plan.transcodesAudio ? t("Re-encoded") : t("Original stream")}</Badge> : null}
        {plan.video?.dynamicRange && plan.video.dynamicRange !== "SDR" ? <Badge tone="accent">{plan.video.dynamicRange}</Badge> : null}
      </div>
      {conflict ? (
        <div className="flex items-start gap-3 rounded-lg border border-warning/30 bg-warning/[0.06] px-3 py-2.5">
          <AlertTriangle className="mt-0.5 size-4 shrink-0 text-warning" />
          <div className="min-w-0 flex-1 text-[13px]">
            <div className="font-medium text-fg">{t("{container} cannot keep these streams", { container: containerLabel(conflict.requested) ?? "" })}</div>
            <div className="mt-0.5 text-fg-2">{tr(conflict.reason)}</div>
            <div className="mt-2 flex flex-wrap gap-2">
              <Button size="sm" variant="primary" onClick={() => onChange({ container: conflict.suggested })}>
                {t("Save as {container}", { container: containerLabel(conflict.suggested) ?? "" })}
              </Button>
              {conflict.requested === "mp4" ? (
                <Button size="sm" variant="secondary" onClick={() => onChange({ qualityMode: "compatible" })}>
                  {t("Use compatible streams instead")}
                </Button>
              ) : null}
            </div>
          </div>
        </div>
      ) : null}
      {plan.notes.length > 0 ? (
        <ul className="space-y-1">
          {plan.notes.map((note) => (
            <li key={note.message} className={cn("flex gap-2 text-xs leading-relaxed", note.level === "warning" ? "text-warning" : "text-fg-2")}>
              {note.level === "warning" ? <AlertTriangle className="mt-0.5 size-3.5 shrink-0" /> : <Info className="mt-0.5 size-3.5 shrink-0" />}
              <span>{tr(note.message)}</span>
            </li>
          ))}
        </ul>
      ) : null}
    </div>
  );
}

export function PlaylistPicker({
  playlist,
  selected,
  onSelectedChange,
  height = 420,
}: {
  playlist: PlaylistInfo;
  selected: Set<number>;
  onSelectedChange: (selected: Set<number>) => void;
  height?: number;
}) {
  const t = useT();
  const parentRef = useRef<HTMLDivElement>(null);
  const entries = playlist.entries;
  const available = entries.filter((e) => e.available);
  const virtualizer = useVirtualizer({ count: entries.length, getScrollElement: () => parentRef.current, estimateSize: () => 60, overscan: 8 });
  const allSelected = available.length > 0 && available.every((e) => selected.has(e.index));
  const someSelected = !allSelected && available.some((e) => selected.has(e.index));

  const toggle = (entry: PlaylistEntry) => {
    if (!entry.available) return;
    const next = new Set(selected);
    if (next.has(entry.index)) next.delete(entry.index);
    else next.add(entry.index);
    onSelectedChange(next);
  };

  return (
    <div className="overflow-hidden rounded-lg border border-line">
      <div className="flex items-center justify-between gap-3 border-b border-line bg-elevated px-3 py-2">
        <label className="flex cursor-pointer items-center gap-2.5 text-[13px] text-fg">
          <Checkbox
            checked={allSelected ? true : someSelected ? "indeterminate" : false}
            onCheckedChange={(v) => onSelectedChange(v === true ? new Set(available.map((e) => e.index)) : new Set())}
            aria-label={t("Select all entries")}
          />
          {t("{selected} of {total} selected", { selected: selected.size, total: available.length })}
        </label>
        {entries.length > available.length ? <span className="text-xs text-fg-2">{t("{count} unavailable", { count: entries.length - available.length })}</span> : null}
      </div>
      <div ref={parentRef} style={{ height: Math.min(height, entries.length * 60 + 4) }} className="overflow-y-auto">
        <div style={{ height: virtualizer.getTotalSize(), position: "relative" }}>
          {virtualizer.getVirtualItems().map((row) => {
            const entry = entries[row.index];
            if (!entry) return null;
            return (
              <div
                key={entry.index}
                style={{ position: "absolute", top: 0, left: 0, width: "100%", height: row.size, transform: `translateY(${row.start}px)` }}
                className={cn("flex items-center gap-3 border-b border-line px-3", entry.available ? "cursor-pointer hover:bg-card" : "opacity-40")}
                onClick={() => toggle(entry)}
              >
                <Checkbox checked={selected.has(entry.index)} disabled={!entry.available} aria-label={t("Select {title}", { title: entry.title })} onClick={(e) => e.stopPropagation()} onCheckedChange={() => toggle(entry)} />
                <span className="tabular w-7 shrink-0 text-right text-xs text-muted">{entry.index}</span>
                <Thumb remote={entry.thumbnail} local={entry.media?.thumbnailLocal} audio={playlist.platform === "spotify"} className={cn("h-10 shrink-0", playlist.platform === "spotify" ? "aspect-square" : "aspect-video")} />
                <div className="min-w-0 flex-1">
                  <div className="truncate text-[13px] text-fg">{entry.title}</div>
                  <div className="truncate text-xs text-fg-2">{entry.uploader ?? (entry.available ? "" : t("Unavailable"))}</div>
                </div>
                <span className="tabular shrink-0 text-xs text-fg-2">{formatDuration(entry.duration)}</span>
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
}
