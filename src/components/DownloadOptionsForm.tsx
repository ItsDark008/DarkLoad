import { useId, useState } from "react";
import { ChevronDown, Film, Music, SlidersHorizontal } from "lucide-react";
import { Input, Label } from "@/components/ui/input";
import { Select, type SelectOption } from "@/components/ui/select";
import { Segmented, Switch } from "@/components/ui/controls";
import { AUDIO_BITRATES, AUDIO_FORMATS, CONTAINERS, QUALITY_MODES } from "@/lib/options";
import { cn } from "@/lib/utils";
import { useApp } from "@/stores/app";
import type { AudioCodecPreference, DownloadOptions, DuplicatePolicy, FormatSummary, VideoCodecPreference } from "@/types/api";

interface Props {
  options: DownloadOptions;
  onChange: (patch: Partial<DownloadOptions>) => void;
  /** Formats offered by the analyzed source. When absent, only source-independent choices are shown. */
  summary?: FormatSummary | null;
  /** Hides global settings (speed limit, duplicates) in per-item editors. */
  showGlobal?: boolean;
  defaultAdvancedOpen?: boolean;
}

function Field({ label, htmlFor, children, hint, className }: { label: string; htmlFor?: string; children: React.ReactNode; hint?: string; className?: string }) {
  return (
    <div className={cn("flex min-w-0 flex-col gap-1.5", className)}>
      <Label htmlFor={htmlFor}>{label}</Label>
      {children}
      {hint ? <p className="text-[11px] leading-snug text-muted">{hint}</p> : null}
    </div>
  );
}

function ToggleRow({ label, description, checked, onCheckedChange, disabled }: { label: string; description?: string; checked: boolean; onCheckedChange: (v: boolean) => void; disabled?: boolean }) {
  const id = useId();
  return (
    <div className="flex items-start justify-between gap-4 py-2">
      <label htmlFor={id} className={cn("min-w-0 cursor-pointer", disabled && "opacity-50")}>
        <div className="text-[13px] text-fg">{label}</div>
        {description ? <div className="mt-0.5 text-xs text-fg-2">{description}</div> : null}
      </label>
      <Switch id={id} checked={checked} onCheckedChange={onCheckedChange} disabled={disabled} />
    </div>
  );
}

const VIDEO_CODECS: SelectOption<VideoCodecPreference>[] = [
  { value: "any", label: "No preference" },
  { value: "av1", label: "AV1" },
  { value: "vp9", label: "VP9" },
  { value: "h265", label: "H.265 / HEVC" },
  { value: "h264", label: "H.264" },
];

const AUDIO_CODECS: SelectOption<AudioCodecPreference>[] = [
  { value: "any", label: "No preference" },
  { value: "opus", label: "Opus" },
  { value: "aac", label: "AAC" },
];

export function DownloadOptionsForm({ options, onChange, summary, showGlobal = true, defaultAdvancedOpen = false }: Props) {
  const settings = useApp((s) => s.settings);
  const updateSettings = useApp((s) => s.updateSettings);
  const [advanced, setAdvanced] = useState(defaultAdvancedOpen);
  const ids = { quality: useId(), resolution: useId(), fps: useId(), container: useId(), custom: useId(), audioFormat: useId(), bitrate: useId(), template: useId(), langs: useId(), speed: useId() };

  const video = options.mediaType === "video";
  const resolutions = summary?.resolutions ?? [];
  const selectedResolution = resolutions.find((r) => r.height === options.targetHeight) ?? null;
  const audioFormat = AUDIO_FORMATS.find((f) => f.value === options.audioFormat);
  const bitrateApplies = !video && options.audioFormat !== "original" && !audioFormat?.lossless;
  const canExact = !summary || resolutions.length > 0;

  const qualityOptions: SelectOption<DownloadOptions["qualityMode"]>[] = QUALITY_MODES.map((m) => ({
    value: m.value,
    label: m.label,
    description: m.description,
    disabled: m.value === "exact" && !canExact,
  }));

  const bitrateValue = options.audioBitrate == null ? "best" : AUDIO_BITRATES.includes(options.audioBitrate as (typeof AUDIO_BITRATES)[number]) ? String(options.audioBitrate) : "custom";

  return (
    <div className="space-y-4">
      <Segmented
        aria-label="Media type"
        value={options.mediaType}
        onValueChange={(mediaType) => onChange({ mediaType })}
        options={[
          { value: "video", label: "Video", icon: <Film /> },
          { value: "audio", label: "Audio", icon: <Music />, disabled: summary ? !summary.hasAudio : false },
        ]}
        className="w-full"
      />

      {video ? (
        <div className="grid grid-cols-2 gap-3">
          <Field label="Quality" htmlFor={ids.quality} className="col-span-2">
            <Select
              id={ids.quality}
              value={options.qualityMode}
              options={qualityOptions}
              onValueChange={(qualityMode) => {
                const patch: Partial<DownloadOptions> = { qualityMode };
                if (qualityMode === "exact" && options.targetHeight == null && resolutions[0]) patch.targetHeight = resolutions[0].height;
                onChange(patch);
              }}
            />
          </Field>
          {options.qualityMode === "exact" ? (
            summary ? (
              <>
                <Field label="Resolution" htmlFor={ids.resolution}>
                  <Select
                    id={ids.resolution}
                    value={String(options.targetHeight ?? resolutions[0]?.height ?? "")}
                    onValueChange={(v) => onChange({ targetHeight: Number(v), targetFps: null })}
                    options={resolutions.map((r) => ({
                      value: String(r.height),
                      label: `${r.label}${r.hdr ? " · HDR" : ""}`,
                    }))}
                  />
                </Field>
                <Field label="Frame rate" htmlFor={ids.fps}>
                  <Select
                    id={ids.fps}
                    value={options.targetFps == null ? "highest" : String(options.targetFps)}
                    onValueChange={(v) => onChange({ targetFps: v === "highest" ? null : Number(v) })}
                    options={[
                      { value: "highest", label: "Highest available" },
                      ...(selectedResolution?.frameRates ?? []).map((fps) => ({ value: String(fps), label: `${fps} fps` })),
                    ]}
                  />
                </Field>
              </>
            ) : (
              <Field label="Resolution" htmlFor={ids.resolution} className="col-span-2" hint="Analyzed per item; fails clearly if the source does not offer it.">
                <Select
                  id={ids.resolution}
                  value={String(options.targetHeight ?? 1080)}
                  onValueChange={(v) => onChange({ targetHeight: Number(v) })}
                  options={[...new Set([4320, 2160, 1440, 1080, 720, 480, 360, ...(options.targetHeight ? [options.targetHeight] : [])])]
                    .sort((a, b) => b - a)
                    .map((h) => ({ value: String(h), label: `${h}p` }))}
                />
              </Field>
            )
          ) : null}
          {options.qualityMode === "custom" ? (
            <Field label="Format expression" htmlFor={ids.custom} className="col-span-2" hint="Passed to yt-dlp as -f. Example: 137+140 or bv*[height<=1080]+ba">
              <Input
                id={ids.custom}
                value={options.customFormat ?? ""}
                maxLength={200}
                placeholder="bv*+ba/b"
                onChange={(e) => onChange({ customFormat: e.target.value })}
                className="font-mono"
              />
            </Field>
          ) : null}
          <Field label="Container" htmlFor={ids.container} className="col-span-2">
            <Select
              id={ids.container}
              value={options.container}
              onValueChange={(container) => onChange({ container })}
              options={CONTAINERS.map((c) => ({ value: c.value, label: c.label, description: c.description }))}
            />
          </Field>
        </div>
      ) : (
        <div className="grid grid-cols-2 gap-3">
          <Field label="Format" htmlFor={ids.audioFormat}>
            <Select
              id={ids.audioFormat}
              value={options.audioFormat}
              onValueChange={(audioFormat) => onChange({ audioFormat })}
              options={AUDIO_FORMATS.map((f) => ({ value: f.value, label: f.label, description: f.lossless ? "Lossless container" : f.value === "original" ? "No re-encoding" : undefined }))}
            />
          </Field>
          <Field label="Bitrate" htmlFor={ids.bitrate}>
            <Select
              id={ids.bitrate}
              disabled={!bitrateApplies}
              value={bitrateApplies ? bitrateValue : "best"}
              onValueChange={(v) => onChange({ audioBitrate: v === "best" ? null : v === "custom" ? (options.audioBitrate ?? 160) : Number(v) })}
              options={[
                { value: "best", label: "Best available" },
                ...AUDIO_BITRATES.map((b) => ({ value: String(b), label: `${b} kbps` })),
                { value: "custom", label: "Custom…" },
              ]}
            />
          </Field>
          {bitrateApplies && bitrateValue === "custom" ? (
            <Field label="Custom bitrate (kbps)" className="col-span-2">
              <Input
                type="number"
                min={32}
                max={512}
                value={options.audioBitrate ?? 160}
                onChange={(e) => {
                  const n = Number(e.target.value);
                  if (Number.isFinite(n)) onChange({ audioBitrate: Math.min(512, Math.max(32, Math.round(n))) });
                }}
              />
            </Field>
          ) : null}
        </div>
      )}

      <div className="rounded-lg border border-line">
        <button
          type="button"
          onClick={() => setAdvanced((v) => !v)}
          aria-expanded={advanced}
          className="flex w-full items-center justify-between gap-2 px-3 py-2.5 text-[13px] font-medium text-fg-2 transition-colors hover:text-fg"
        >
          <span className="flex items-center gap-2">
            <SlidersHorizontal className="size-4" />
            Advanced options
          </span>
          <ChevronDown className={cn("size-4 transition-transform", advanced && "rotate-180")} />
        </button>
        {advanced ? (
          <div className="space-y-1 border-t border-line px-3 pt-3 pb-2">
            {video ? (
              <div className="grid grid-cols-2 gap-3 pb-2">
                <Field label="Video codec preference">
                  <Select aria-label="Video codec preference" value={options.videoCodec} onValueChange={(videoCodec) => onChange({ videoCodec })} options={VIDEO_CODECS} />
                </Field>
                <Field label="Audio codec preference">
                  <Select aria-label="Audio codec preference" value={options.audioCodec} onValueChange={(audioCodec) => onChange({ audioCodec })} options={AUDIO_CODECS} />
                </Field>
              </div>
            ) : (
              <div className="pb-2">
                <Field label="Source audio preference" hint="Used when several audio streams have similar quality.">
                  <Select aria-label="Source audio preference" value={options.audioCodec} onValueChange={(audioCodec) => onChange({ audioCodec })} options={AUDIO_CODECS} />
                </Field>
              </div>
            )}
            <div className="divide-y divide-line">
              {video ? (
                <ToggleRow
                  label="Allow HDR"
                  description="Keep high dynamic range when the source offers it."
                  checked={options.allowHdr}
                  onCheckedChange={(allowHdr) => onChange({ allowHdr })}
                />
              ) : null}
              <ToggleRow label="Embed metadata" description="Title, artist or uploader, date and source link." checked={options.embedMetadata} onCheckedChange={(embedMetadata) => onChange({ embedMetadata })} />
              <ToggleRow label="Embed thumbnail" description="Cover art inside the file where the format supports it." checked={options.embedThumbnail} onCheckedChange={(embedThumbnail) => onChange({ embedThumbnail })} />
              <ToggleRow label="Save thumbnail file" description="Also save the thumbnail image next to the media." checked={options.writeThumbnail} onCheckedChange={(writeThumbnail) => onChange({ writeThumbnail })} />
              <ToggleRow label="Embed chapters" checked={options.embedChapters} onCheckedChange={(embedChapters) => onChange({ embedChapters })} />
              {video ? (
                <>
                  <ToggleRow label="Download subtitles" checked={options.downloadSubtitles} onCheckedChange={(downloadSubtitles) => onChange({ downloadSubtitles })} />
                  {options.downloadSubtitles ? (
                    <>
                      <ToggleRow label="Embed subtitles" description="Store subtitles inside MP4, MKV or WebM." checked={options.embedSubtitles} onCheckedChange={(embedSubtitles) => onChange({ embedSubtitles })} />
                      <ToggleRow label="Include automatic captions" checked={options.includeAutoSubtitles} onCheckedChange={(includeAutoSubtitles) => onChange({ includeAutoSubtitles })} />
                      <div className="py-2">
                        <Field label="Subtitle languages" htmlFor={ids.langs} hint="Comma-separated codes, for example en, de, pt-BR, or all.">
                          <Input
                            id={ids.langs}
                            defaultValue={options.subtitleLanguages.join(", ")}
                            onBlur={(e) => {
                              const langs = e.target.value
                                .split(",")
                                .map((l) => l.trim())
                                .filter((l) => /^[A-Za-z0-9_-]{1,16}$/.test(l))
                                .slice(0, 20);
                              onChange({ subtitleLanguages: langs.length > 0 ? langs : ["en"] });
                            }}
                          />
                        </Field>
                      </div>
                    </>
                  ) : null}
                </>
              ) : null}
            </div>
            <div className="py-2">
              <Field label="Filename template" htmlFor={ids.template} hint="yt-dlp template fields, ending in .%(ext)s. Leave empty to use the default from Settings.">
                <Input
                  id={ids.template}
                  className="font-mono"
                  value={options.filenameTemplate ?? ""}
                  placeholder={settings?.filenameTemplate}
                  maxLength={240}
                  onChange={(e) => onChange({ filenameTemplate: e.target.value.trim() === "" ? null : e.target.value })}
                />
              </Field>
            </div>
            {showGlobal && settings ? (
              <div className="grid grid-cols-2 gap-3 border-t border-line pt-3 pb-2">
                <Field label="Speed limit (KiB/s)" htmlFor={ids.speed} hint="Applies to all downloads. Empty for unlimited.">
                  <Input
                    id={ids.speed}
                    type="number"
                    min={16}
                    placeholder="Unlimited"
                    defaultValue={settings.speedLimitKib ?? ""}
                    onBlur={(e) => {
                      const raw = e.target.value.trim();
                      const value = raw === "" ? null : Math.max(16, Math.round(Number(raw)));
                      if (value !== settings.speedLimitKib && (value == null || Number.isFinite(value))) void updateSettings({ speedLimitKib: value });
                    }}
                  />
                </Field>
                <Field label="When a file exists" hint="Applies to all downloads.">
                  <Select<DuplicatePolicy>
                    aria-label="Duplicate handling"
                    value={settings.duplicatePolicy}
                    onValueChange={(duplicatePolicy) => void updateSettings({ duplicatePolicy })}
                    options={[
                      { value: "rename", label: "Keep both" },
                      { value: "skip", label: "Skip" },
                      { value: "overwrite", label: "Overwrite" },
                    ]}
                  />
                </Field>
              </div>
            ) : null}
          </div>
        ) : null}
      </div>
    </div>
  );
}
