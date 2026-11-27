import { useId, useState, type ReactNode } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { openUrl } from "@tauri-apps/plugin-opener";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { toast } from "sonner";
import {
  Brush,
  CircleArrowUp,
  Download,
  ExternalLink,
  FileCog,
  Loader2,
  Package,
  RefreshCw,
  RotateCcw,
  Shield,
  SlidersHorizontal,
  Stethoscope,
  Wrench,
  type LucideIcon,
} from "lucide-react";
import { PageHeader } from "@/components/common";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Select } from "@/components/ui/select";
import { Badge, Card, Progress, Segmented, Switch, Tooltip } from "@/components/ui/controls";
import { ConfirmDialog, Dialog, DialogContent } from "@/components/ui/overlays";
import { formatBytes, formatDate } from "@/lib/format";
import { AUDIO_BITRATES, audioFormats, containers } from "@/lib/options";
import { t, tr, useT } from "@/i18n";
import { cn, isValidOledBackground } from "@/lib/utils";
import { api } from "@/services/api";
import { useApp, type SettingsSection } from "@/stores/app";
import { useHistorySignal } from "@/stores/signals";
import type { DependencyStatus, DepId, LanguagePreference, Settings } from "@/types/api";
import { OutputFolder } from "./Downloader";
import { Changelog } from "./About";

function sections(): { id: SettingsSection; label: string; icon: LucideIcon }[] {
  return [
    { id: "general", label: t("General"), icon: SlidersHorizontal },
    { id: "downloads", label: t("Downloads"), icon: Download },
    { id: "appearance", label: t("Appearance"), icon: Brush },
    { id: "dependencies", label: t("Dependencies"), icon: Package },
    { id: "updates", label: t("Updates"), icon: CircleArrowUp },
    { id: "privacy", label: t("Privacy"), icon: Shield },
  ];
}

function Row({ label, description, children, htmlFor }: { label: string; description?: ReactNode; children: ReactNode; htmlFor?: string }) {
  return (
    <div className="flex items-center justify-between gap-6 px-5 py-4">
      <div className="min-w-0">
        <label htmlFor={htmlFor} className="text-[13px] font-medium text-fg">
          {label}
        </label>
        {description ? <div className="mt-0.5 text-xs leading-relaxed text-fg-2">{description}</div> : null}
      </div>
      <div className="flex shrink-0 items-center gap-2">{children}</div>
    </div>
  );
}

function Group({ title, children }: { title?: string; children: ReactNode }) {
  return (
    <section className="mb-6">
      {title ? <h2 className="mb-2 px-1 text-xs font-semibold tracking-wide text-fg-2 uppercase">{title}</h2> : null}
      <Card className="divide-y divide-line">{children}</Card>
    </section>
  );
}

function ToggleSetting({ label, description, value, onChange }: { label: string; description?: ReactNode; value: boolean; onChange: (v: boolean) => void }) {
  const id = useId();
  return (
    <Row label={label} description={description} htmlFor={id}>
      <Switch id={id} checked={value} onCheckedChange={onChange} />
    </Row>
  );
}

function NumberSetting({ value, min, max, onCommit, suffix, placeholder, allowEmpty }: { value: number | null; min: number; max: number; onCommit: (v: number | null) => void; suffix?: string; placeholder?: string; allowEmpty?: boolean }) {
  const [editing, setDraft] = useState<string | null>(null);
  const draft = editing ?? (value == null ? "" : String(value));
  const commit = () => {
    setDraft(null);
    const raw = draft.trim();
    if (raw === "" && allowEmpty) {
      if (value !== null) onCommit(null);
      return;
    }
    const n = Math.round(Number(raw));
    if (!Number.isFinite(n) || n < min || n > max) {
      toast.error(t("Enter a value between {min} and {max}.", { min, max }));
      return;
    }
    if (n !== value) onCommit(n);
  };
  return (
    <div className="flex items-center gap-2">
      <Input type="number" className="w-28" min={min} max={max} value={draft} placeholder={placeholder} onChange={(e) => setDraft(e.target.value)} onBlur={commit} onKeyDown={(e) => e.key === "Enter" && commit()} />
      {suffix ? <span className="text-xs text-fg-2">{suffix}</span> : null}
    </div>
  );
}

function General({ s, set }: { s: Settings; set: (p: Partial<Settings>) => Promise<boolean> }) {
  const [templateDraft, setTemplate] = useState<string | null>(null);
  const template = templateDraft ?? s.filenameTemplate;
  const saveTemplate = () => void set({ filenameTemplate: template }).then(() => setTemplate(null));
  return (
    <>
      <Group title={t("Downloads")}>
        <div className="px-5 py-4">
          <div className="text-[13px] font-medium">{t("Default download folder")}</div>
          <div className="mt-0.5 mb-3 text-xs text-fg-2">{t("New downloads are saved here unless you choose another folder.")}</div>
          <OutputFolder value={s.downloadDir} onChange={(downloadDir) => void set({ downloadDir })} />
        </div>
        <Row label={t("Default media type")}>
          <Segmented aria-label={t("Default media type")} size="sm" value={s.defaultMediaType} onValueChange={(defaultMediaType) => void set({ defaultMediaType })} options={[{ value: "video", label: t("Video") }, { value: "audio", label: t("Audio") }]} />
        </Row>
        <Row label={t("Default quality")}>
          <Select aria-label={t("Default quality")} className="w-48" value={s.defaultQualityMode === "compatible" ? "compatible" : "best"} onValueChange={(defaultQualityMode) => void set({ defaultQualityMode })} options={[{ value: "best", label: t("Best available") }, { value: "compatible", label: t("Best compatible") }]} />
        </Row>
        <Row label={t("Default video container")}>
          <Select aria-label={t("Default container")} className="w-48" value={s.defaultContainer} onValueChange={(defaultContainer) => void set({ defaultContainer })} options={containers().map((c) => ({ value: c.value, label: c.label, description: c.description }))} />
        </Row>
        <Row label={t("Default audio format")}>
          <Select aria-label={t("Default audio format")} className="w-48" value={s.defaultAudioFormat} onValueChange={(defaultAudioFormat) => void set({ defaultAudioFormat })} options={audioFormats().map((f) => ({ value: f.value, label: f.label }))} />
        </Row>
        <Row label={t("Default audio bitrate")} description={t("Used when converting to a lossy format.")}>
          <Select aria-label={t("Default audio bitrate")} className="w-48" value={s.defaultAudioBitrate == null ? "best" : String(s.defaultAudioBitrate)} onValueChange={(v) => void set({ defaultAudioBitrate: v === "best" ? null : Number(v) })} options={[{ value: "best", label: t("Best available") }, ...AUDIO_BITRATES.map((b) => ({ value: String(b), label: `${b} kbps` }))]} />
        </Row>
        <div className="px-5 py-4">
          <div className="text-[13px] font-medium">{t("Filename template")}</div>
          <div className="mt-0.5 mb-3 text-xs text-fg-2">
            {t("yt-dlp output template for file names. Folders are not allowed; it must end with")} <code className="font-mono">.%(ext)s</code>.
          </div>
          <div className="flex gap-2">
            <Input className="font-mono" value={template} maxLength={240} onChange={(e) => setTemplate(e.target.value)} onKeyDown={(e) => e.key === "Enter" && saveTemplate()} aria-label={t("Filename template")} />
            <Button disabled={template === s.filenameTemplate} onClick={saveTemplate}>
              {t("Save")}
            </Button>
          </div>
        </div>
      </Group>
      <Group title={t("Application")}>
        <Row label={t("Language")} description={t("The language of DarkLoad's interface and notifications.")}>
          <Select<LanguagePreference>
            aria-label={t("Language")}
            className="w-48"
            value={s.language ?? "system"}
            onValueChange={(language) => void set({ language })}
            options={[
              { value: "system", label: t("System default") },
              { value: "en", label: "English" },
              { value: "it", label: "Italiano" },
            ]}
          />
        </Row>
        <ToggleSetting
          label={t("Use the system window frame")}
          description={t("Replace DarkLoad's title bar with the operating system's. Useful with some Linux window managers.")}
          value={s.nativeWindowFrame}
          onChange={(nativeWindowFrame) => {
            void set({ nativeWindowFrame }).then(async (ok) => {
              if (ok) await getCurrentWindow().setDecorations(nativeWindowFrame);
            });
          }}
        />
        <ToggleSetting label={t("Confirm before closing during downloads")} value={s.confirmExitWithActiveDownloads} onChange={(confirmExitWithActiveDownloads) => void set({ confirmExitWithActiveDownloads })} />
      </Group>
    </>
  );
}

function SpotifyCredentials({ s, set }: { s: Settings; set: (p: Partial<Settings>) => Promise<boolean> }) {
  const [clientId, setClientId] = useState<string | null>(null);
  const [secret, setSecret] = useState<string | null>(null);
  const id = clientId ?? s.spotifyClientId;
  const sec = secret ?? s.spotifyClientSecret;
  const changed = id !== s.spotifyClientId || sec !== s.spotifyClientSecret;
  const configured = s.spotifyClientId !== "" && s.spotifyClientSecret !== "";
  const save = (spotifyClientId: string, spotifyClientSecret: string) =>
    void set({ spotifyClientId: spotifyClientId.trim(), spotifyClientSecret: spotifyClientSecret.trim() }).then((ok) => {
      if (ok) {
        setClientId(null);
        setSecret(null);
      }
    });
  return (
    <Group title="Spotify">
      <div className="px-5 py-4">
        <div className="flex items-center gap-2 text-[13px] font-medium">
          {t("Spotify API credentials")} <Badge>{configured ? t("In use") : t("Optional")}</Badge>
        </div>
        <div className="mt-0.5 mb-3 text-xs leading-relaxed text-fg-2">
          {t("Without credentials DarkLoad reads Spotify's public player, which lists at most 100 songs of an album or playlist and no album name for single songs. With the Client ID and Client Secret of your own app from the Spotify Developer Dashboard, long playlists are read in full. Spotify's own editorial playlists are still read from the public player. The credentials stay on this computer.")}
        </div>
        <div className="grid grid-cols-2 gap-2">
          <Input className="font-mono" value={id} maxLength={64} placeholder="Client ID" autoComplete="off" spellCheck={false} onChange={(e) => setClientId(e.target.value)} aria-label="Spotify Client ID" />
          <Input className="font-mono" type="password" value={sec} maxLength={64} placeholder="Client Secret" autoComplete="off" spellCheck={false} onChange={(e) => setSecret(e.target.value)} aria-label="Spotify Client Secret" />
        </div>
        <div className="mt-3 flex justify-end gap-2">
          {configured ? (
            <Button variant="ghost" onClick={() => save("", "")}>
              {t("Remove")}
            </Button>
          ) : null}
          <Button disabled={!changed} onClick={() => save(id, sec)}>
            {t("Save")}
          </Button>
        </div>
      </div>
    </Group>
  );
}

function Downloads({ s, set }: { s: Settings; set: (p: Partial<Settings>) => Promise<boolean> }) {
  return (
    <>
      <Group title={t("Queue")}>
        <Row label={t("Concurrent downloads")} description={t("How many downloads run at the same time.")}>
          <Select aria-label={t("Concurrent downloads")} className="w-28" value={String(s.maxConcurrentDownloads)} onValueChange={(v) => void set({ maxConcurrentDownloads: Number(v) })} options={Array.from({ length: 8 }, (_, i) => ({ value: String(i + 1), label: String(i + 1) }))} />
        </Row>
        <ToggleSetting label={t("Resume the queue on startup")} description={t("Unfinished downloads continue automatically when DarkLoad starts.")} value={s.resumeQueueOnStartup} onChange={(resumeQueueOnStartup) => void set({ resumeQueueOnStartup })} />
        <Row label={t("Playlists")} description={t("Which entries are selected when a playlist is analyzed.")}>
          <Select aria-label={t("Playlist behavior")} className="w-48" value={s.playlistBehavior} onValueChange={(playlistBehavior) => void set({ playlistBehavior })} options={[{ value: "all", label: t("Select all entries") }, { value: "select", label: t("Let me choose") }]} />
        </Row>
      </Group>
      <Group title={t("Network")}>
        <Row label={t("Speed limit")} description={t("Per download. Leave empty for unlimited.")}>
          <NumberSetting value={s.speedLimitKib} min={16} max={10_000_000} allowEmpty placeholder={t("Unlimited")} suffix="KiB/s" onCommit={(speedLimitKib) => void set({ speedLimitKib })} />
        </Row>
        <Row label={t("Retry attempts")} description={t("Automatic retries for network interruptions and server errors.")}>
          <NumberSetting value={s.retryAttempts} min={0} max={10} onCommit={(v) => void set({ retryAttempts: v ?? 3 })} />
        </Row>
        <Row label={t("Network timeout")}>
          <NumberSetting value={s.socketTimeoutSecs} min={5} max={600} suffix={t("seconds")} onCommit={(v) => void set({ socketTimeoutSecs: v ?? 30 })} />
        </Row>
        <ToggleSetting
          label={t("Prefer IPv4")}
          description={t("Connect over IPv4 only. Prevents long stalls on networks whose IPv6 connectivity is broken.")}
          value={s.preferIpv4}
          onChange={(preferIpv4) => void set({ preferIpv4 })}
        />
        <Row label={t("Parallel fragments")} description={t("Fragments fetched in parallel for segmented streams.")}>
          <NumberSetting value={s.concurrentFragments} min={1} max={16} onCommit={(v) => void set({ concurrentFragments: v ?? 4 })} />
        </Row>
      </Group>
      <Group title={t("Files")}>
        <Row label={t("When a file already exists")}>
          <Select aria-label={t("Duplicate handling")} className="w-48" value={s.duplicatePolicy} onValueChange={(duplicatePolicy) => void set({ duplicatePolicy })} options={[{ value: "rename", label: t("Keep both (rename)") }, { value: "skip", label: t("Skip the new file") }, { value: "overwrite", label: t("Overwrite") }]} />
        </Row>
        <Row label={t("Partial files")} description={t("What happens to incomplete data when a download is cancelled or fails.")}>
          <Select aria-label={t("Partial files")} className="w-48" value={s.partialFiles} onValueChange={(partialFiles) => void set({ partialFiles })} options={[{ value: "delete", label: t("Delete") }, { value: "keep", label: t("Keep to resume later") }]} />
        </Row>
      </Group>
      <SpotifyCredentials s={s} set={set} />
    </>
  );
}

function oledBackgrounds(): { value: string; name: string }[] {
  return [
    { value: "#000000", name: t("Pure black") },
    { value: "#04060E", name: t("Midnight") },
    { value: "#0A0510", name: t("Plum") },
    { value: "#030A06", name: t("Forest") },
    { value: "#0E0505", name: t("Ember") },
    { value: "#0C0C0E", name: t("Graphite") },
  ];
}

function OledBackgroundPicker({ s, set }: { s: Settings; set: (p: Partial<Settings>) => Promise<boolean> }) {
  const [draft, setDraft] = useState<string | null>(null);
  const value = draft ?? s.oledBackground;
  const current = s.oledBackground.toUpperCase();
  const invalid = draft != null && draft.length === 7 && !isValidOledBackground(draft);
  const commit = () => {
    if (draft == null) return;
    setDraft(null);
    const next = draft.toUpperCase();
    if (next === current) return;
    if (isValidOledBackground(next)) void set({ oledBackground: next });
    else toast.error(t("Choose a darker color"), { description: t("The background must stay very dark so text remains readable.") });
  };
  return (
    <div className="px-5 py-4">
      <div className="text-[13px] font-medium">{t("Background")}</div>
      <div className="mt-0.5 text-xs text-fg-2">
        {t("The base color of the OLED theme. Cards, borders and menus are derived from it. Pure black turns pixels off on OLED displays.")}
      </div>
      <div className="mt-3 flex flex-wrap items-center gap-2">
        {oledBackgrounds().map((option) => {
          const selected = current === option.value;
          return (
            <Tooltip key={option.value} content={`${option.name} · ${option.value}`}>
              <button
                type="button"
                aria-label={t("{name} background", { name: option.name })}
                aria-pressed={selected}
                onClick={() => void set({ oledBackground: option.value })}
                className={cn(
                  "size-8 rounded-lg border-2 transition-transform hover:scale-110",
                  selected ? "border-accent-hover" : "border-line-strong",
                )}
                style={{ background: option.value }}
              />
            </Tooltip>
          );
        })}
        <Input
          aria-label={t("Custom background color")}
          aria-invalid={invalid}
          className="ml-2 w-28 font-mono uppercase"
          value={value}
          maxLength={7}
          onChange={(e) => setDraft(e.target.value)}
          onBlur={commit}
          onKeyDown={(e) => e.key === "Enter" && commit()}
        />
        {current !== "#000000" ? (
          <Button size="sm" variant="ghost" onClick={() => void set({ oledBackground: "#000000" })}>
            {t("Reset")}
          </Button>
        ) : null}
      </div>
      {invalid ? <p className="mt-2 text-xs text-danger">{t("This color is too bright for the OLED theme.")}</p> : null}
    </div>
  );
}

const ACCENTS = ["#8B5CF6", "#6366F1", "#3B82F6", "#06B6D4", "#10B981", "#EAB308", "#F97316", "#EC4899"];

function Appearance({ s, set }: { s: Settings; set: (p: Partial<Settings>) => Promise<boolean> }) {
  const [hexDraft, setHex] = useState<string | null>(null);
  const hex = hexDraft ?? s.accentColor;
  return (
    <>
      <Group title={t("Theme")}>
        <Row label={t("Theme")} description={t("OLED uses true-black surfaces that save power on OLED displays.")}>
          <Segmented aria-label={t("Theme")} size="sm" value={s.theme} onValueChange={(theme) => void set({ theme })} options={[{ value: "oled", label: t("OLED black") }, { value: "dark", label: t("Dark") }]} />
        </Row>
        {s.theme === "oled" ? <OledBackgroundPicker s={s} set={set} /> : null}
        <div className="px-5 py-4">
          <div className="text-[13px] font-medium">{t("Accent color")}</div>
          <div className="mt-3 flex flex-wrap items-center gap-2">
            {ACCENTS.map((c) => (
              <button
                key={c}
                type="button"
                aria-label={t("Accent {color}", { color: c })}
                aria-pressed={s.accentColor.toUpperCase() === c}
                onClick={() => void set({ accentColor: c })}
                className={cn("size-7 rounded-full border-2 transition-transform hover:scale-110", s.accentColor.toUpperCase() === c ? "border-white" : "border-transparent")}
                style={{ background: c }}
              />
            ))}
            <Input
              aria-label={t("Custom accent color")}
              className="ml-2 w-28 font-mono uppercase"
              value={hex}
              maxLength={7}
              onChange={(e) => setHex(e.target.value)}
              onBlur={() => {
                setHex(null);
                if (/^#[0-9a-f]{6}$/i.test(hex) && hex.toUpperCase() !== s.accentColor.toUpperCase()) void set({ accentColor: hex.toUpperCase() });
              }}
            />
            {s.accentColor.toUpperCase() !== "#8B5CF6" ? (
              <Button size="sm" variant="ghost" onClick={() => void set({ accentColor: "#8B5CF6" })}>
                {t("Reset")}
              </Button>
            ) : null}
          </div>
        </div>
      </Group>
      <Group title={t("Layout")}>
        <Row label={t("Sidebar")} description={t("Collapse automatically on narrow windows or remember your choice.")}>
          <Select aria-label={t("Sidebar behavior")} className="w-48" value={s.sidebarBehavior} onValueChange={(sidebarBehavior) => void set({ sidebarBehavior })} options={[{ value: "remember", label: t("Remember state") }, { value: "auto", label: t("Collapse automatically") }]} />
        </Row>
        {s.sidebarBehavior === "remember" ? <ToggleSetting label={t("Collapse sidebar")} value={s.sidebarCollapsed} onChange={(sidebarCollapsed) => void set({ sidebarCollapsed })} /> : null}
        <Row label={t("Interface density")}>
          <Segmented aria-label={t("Density")} size="sm" value={s.density} onValueChange={(density) => void set({ density })} options={[{ value: "comfortable", label: t("Comfortable") }, { value: "compact", label: t("Compact") }]} />
        </Row>
        <ToggleSetting label={t("Animations")} description={t("Reduced motion from your system settings is always respected.")} value={s.animations} onChange={(animations) => void set({ animations })} />
      </Group>
    </>
  );
}

function stageLabel(stage: string): string {
  switch (stage) {
    case "resolving":
      return t("Finding latest release");
    case "downloading":
      return t("Downloading");
    case "verifying":
      return t("Verifying checksum");
    case "extracting":
      return t("Extracting");
    case "validating":
      return t("Validating");
    case "done":
      return t("Installed");
    case "failed":
      return t("Failed");
    default:
      return stage;
  }
}

function DependencyCard({ dep }: { dep: DependencyStatus }) {
  const progress = useApp((s) => s.installProgress[dep.id === "ffprobe" ? "ffmpeg" : dep.id]);
  const installing = useApp((s) => s.installing);
  const fraction = progress?.total ? progress.downloaded / progress.total : null;
  return (
    <div className="px-5 py-4">
      <div className="flex items-start justify-between gap-4">
        <div className="min-w-0">
          <div className="flex flex-wrap items-center gap-2">
            <span className="text-[13px] font-medium">{dep.name}</span>
            {dep.available ? (
              <Badge tone="success">{t("Available")}</Badge>
            ) : installing && progress?.stage === "done" ? (
              <Badge tone="success">{t("Installed")}</Badge>
            ) : (
              <Badge tone={dep.required ? "danger" : "warning"}>{dep.required ? t("Missing") : t("Not installed")}</Badge>
            )}
            {dep.source === "system" ? <Badge>{t("System")}</Badge> : dep.source === "managed" ? <Badge>{t("Managed")}</Badge> : null}
            {!dep.required ? <Badge>{t("Recommended")}</Badge> : null}
            {dep.updateAvailable ? <Badge tone="accent">{t("Update:")} {dep.latestVersion}</Badge> : null}
          </div>
          <div className="mt-1 text-xs text-fg-2">{tr(dep.description)}</div>
          {dep.error ? <div className="mt-1 text-xs text-danger" data-selectable>{tr(dep.error)}</div> : null}
        </div>
        <div className="shrink-0 text-right">
          <div className="max-w-[260px] truncate font-mono text-[12px] text-fg" data-selectable title={dep.version ?? undefined}>
            {dep.version ?? "—"}
          </div>
          {dep.installedAt ? <div className="text-[11px] text-muted">{t("Installed {date}", { date: formatDate(dep.installedAt) })}</div> : null}
        </div>
      </div>
      {dep.path ? (
        <div className="mt-2 truncate font-mono text-[11px] text-muted" data-selectable title={dep.path}>
          {dep.path}
        </div>
      ) : null}
      {installing && progress && progress.stage !== "done" && dep.id !== "ffprobe" ? (
        <div className="mt-3 space-y-1.5">
          <Progress value={progress.stage === "downloading" ? fraction : progress.stage === "failed" ? 0 : null} tone={progress.stage === "failed" ? "danger" : "accent"} />
          <div className="tabular text-[11px] text-fg-2">
            {stageLabel(progress.stage)}
            {progress.stage === "downloading"
              ? ` · ${progress.total ? t("{done} of {total}", { done: formatBytes(progress.downloaded), total: formatBytes(progress.total) }) : formatBytes(progress.downloaded)}`
              : ""}
          </div>
        </div>
      ) : null}
    </div>
  );
}

function Dependencies({ s, set }: { s: Settings; set: (p: Partial<Settings>) => Promise<boolean> }) {
  const deps = useApp((st) => st.deps);
  const setDeps = useApp((st) => st.setDeps);
  const installDeps = useApp((st) => st.installDeps);
  const installing = useApp((st) => st.installing);
  const [checking, setChecking] = useState(false);
  const [diagnostics, setDiagnostics] = useState<string | null>(null);
  const [repairOpen, setRepairOpen] = useState(false);
  const missing = deps.filter((d) => !d.available).map((d) => d.id);
  const outdated = deps.filter((d) => d.updateAvailable).map((d) => d.id);

  const check = async () => {
    setChecking(true);
    try {
      const result = await api.depsCheckUpdates();
      setDeps(result);
      const n = result.filter((d) => d.updateAvailable).length;
      toast.success(n ? (n === 1 ? t("1 update available") : t("{count} updates available", { count: n })) : t("All components are up to date"));
    } catch (e) {
      toast.error(t("Could not check for updates"), { description: (e as { message?: string }).message });
    } finally {
      setChecking(false);
    }
  };

  return (
    <>
      <Group>
        <div className="flex flex-wrap items-center gap-2 px-5 py-4">
          {missing.length > 0 ? (
            <Button variant="primary" disabled={installing} onClick={() => void installDeps(missing as DepId[])}>
              {installing ? <Loader2 className="animate-spin" /> : <Download />} {t("Install missing")}
            </Button>
          ) : null}
          {outdated.length > 0 ? (
            <Button variant="primary" disabled={installing} onClick={() => void installDeps(outdated as DepId[])}>
              <CircleArrowUp /> {t("Update {count}", { count: outdated.length })}
            </Button>
          ) : null}
          <Button disabled={checking || installing} onClick={() => void check()}>
            {checking ? <Loader2 className="animate-spin" /> : <RefreshCw />} {t("Check for updates")}
          </Button>
          <Button variant="ghost" disabled={installing} onClick={() => setRepairOpen(true)}>
            <Wrench /> {t("Repair")}
          </Button>
          <Button variant="ghost" onClick={() => void api.depsDiagnostics().then(setDiagnostics)}>
            <Stethoscope /> {t("Diagnostics")}
          </Button>
        </div>
        {deps.map((d) => (
          <DependencyCard key={d.id} dep={d} />
        ))}
      </Group>
      <Group title={t("Sources")}>
        <div className="px-5 py-4 text-xs leading-relaxed text-fg-2">
          {t("Components are downloaded only when you ask, from their official GitHub releases (yt-dlp/yt-dlp, yt-dlp/FFmpeg-Builds and denoland/deno), and installed only after their published SHA-256 checksums match. They are stored in DarkLoad's own data folder.")}
        </div>
        <ToggleSetting
          label={t("Use tools installed on this system")}
          description={t("Fall back to yt-dlp, FFmpeg or Deno found on PATH when no managed copy exists. Off by default for predictable results.")}
          value={s.allowSystemBinaries}
          onChange={(allowSystemBinaries) => void set({ allowSystemBinaries })}
        />
      </Group>
      <ConfirmDialog
        open={repairOpen}
        onOpenChange={setRepairOpen}
        title={t("Repair components?")}
        description={t("DarkLoad re-downloads and re-verifies yt-dlp, FFmpeg and Deno from their official releases, replacing the current copies.")}
        confirmLabel={t("Repair")}
        onConfirm={() => void installDeps(["yt-dlp", "ffmpeg", "deno"], true)}
      />
      <Dialog open={diagnostics != null} onOpenChange={(o) => !o && setDiagnostics(null)}>
        <DialogContent
          title={t("Diagnostics")}
          description={t("Share this report when asking for help. It contains no personal data beyond file paths.")}
          className="w-[min(760px,calc(100vw-48px))]"
          footer={
            <Button variant="primary" onClick={() => diagnostics && void writeText(diagnostics).then(() => toast.success(t("Diagnostics copied")))}>
              {t("Copy report")}
            </Button>
          }
        >
          <pre className="max-h-[50vh] overflow-auto rounded-lg border border-line bg-bg p-3 font-mono text-[11px] leading-relaxed whitespace-pre-wrap text-fg-2" data-selectable>
            {diagnostics}
          </pre>
        </DialogContent>
      </Dialog>
    </>
  );
}

function Updates({ s, set }: { s: Settings; set: (p: Partial<Settings>) => Promise<boolean> }) {
  const info = useApp((st) => st.info);
  const update = useApp((st) => st.update);
  const setUpdate = useApp((st) => st.setUpdate);
  const [checking, setChecking] = useState(false);
  const [installing, setInstalling] = useState(false);
  const [message, setMessage] = useState<string | null>(null);

  const check = async () => {
    setChecking(true);
    setMessage(null);
    try {
      const result = await api.checkAppUpdate(true);
      setUpdate(result);
      setMessage(result.available ? null : t("DarkLoad {version} is the latest version.", { version: result.currentVersion }));
    } catch (e) {
      setMessage((e as { message?: string }).message ?? t("Could not check for updates."));
    } finally {
      setChecking(false);
    }
  };

  return (
    <>
      <Group title={t("Application")}>
        <Row label={t("Current version")}>
          <span className="tabular font-mono text-[13px]">{info?.version}</span>
        </Row>
        <Row label={t("Automatic updates")} description={t("DarkLoad checks at startup and every 6 hours. Only the version information is requested; nothing about you is sent. Every update is verified against DarkLoad's signing key before it is installed.")}>
          <Select
            aria-label={t("Automatic updates")}
            className="w-56"
            value={s.updateMode}
            onValueChange={(updateMode) => void set({ updateMode })}
            options={[
              { value: "auto", label: t("Download and install"), description: t("Installs when you close DarkLoad, or restart from the notification.") },
              { value: "notify", label: t("Only notify me"), description: t("Tells you about new versions; you decide when to install.") },
              { value: "off", label: t("Off"), description: t("Never checks in the background.") },
            ]}
          />
        </Row>
        <div className="px-5 py-4">
          <div className="flex flex-wrap items-center gap-2">
            <Button disabled={checking} onClick={() => void check()}>
              {checking ? <Loader2 className="animate-spin" /> : <RefreshCw />} {t("Check now")}
            </Button>
            {update?.available && update.installable ? (
              <Button
                variant="primary"
                disabled={installing}
                onClick={() => {
                  setInstalling(true);
                  api
                    .installAppUpdate()
                    .catch((e: { message?: string }) => toast.error(t("Update failed"), { description: e.message }))
                    .finally(() => setInstalling(false));
                }}
              >
                {installing ? <Loader2 className="animate-spin" /> : <CircleArrowUp />} {update.ready ? t("Restart and install") : t("Install")} {update.version}
              </Button>
            ) : null}
            {update?.available && !update.installable && info?.repository ? (
              <Button variant="primary" onClick={() => void openUrl(`https://github.com/${info.repository}/releases/latest`)}>
                <ExternalLink /> {t("Download")} {update.version}
              </Button>
            ) : null}
          </div>
          {update?.available && update.ready ? <p className="mt-3 text-xs text-fg-2">{t("DarkLoad {version} is downloaded and verified. It installs when you close DarkLoad.", { version: update.version ?? "" })}</p> : null}
          {update?.available && !update.installable ? <p className="mt-3 text-xs text-fg-2">{t("This portable copy cannot update itself. Download the new version from the releases page, or use the installer to get automatic updates.")}</p> : null}
          {message ? <p className="mt-3 text-xs text-fg-2">{message}</p> : null}
          {!info?.updaterConfigured ? <p className="mt-3 text-xs text-fg-2">{t("This build was compiled without an update channel. New versions are available from the project's releases page.")}</p> : null}
          {update?.available && update.notes ? (
            <pre className="mt-4 max-h-56 overflow-auto rounded-lg border border-line bg-bg p-3 font-sans text-xs leading-relaxed whitespace-pre-wrap text-fg-2" data-selectable>
              {update.notes}
            </pre>
          ) : null}
        </div>
      </Group>
      <Group title={t("Release changelog")}>
        <div className="max-h-[420px] overflow-y-auto px-5 py-4">{info ? <Changelog markdown={info.changelog} /> : null}</div>
      </Group>
    </>
  );
}

function Privacy({ s, set }: { s: Settings; set: (p: Partial<Settings>) => Promise<boolean> }) {
  const setSettings = useApp((st) => st.setSettings);
  const bump = useHistorySignal((st) => st.bump);
  const [confirm, setConfirm] = useState<null | "history" | "reset">(null);
  return (
    <>
      <Group title={t("Privacy")}>
        <div className="flex items-start gap-3 px-5 py-4 text-xs leading-relaxed text-fg-2">
          <Shield className="mt-0.5 size-4 shrink-0 text-success" />
          {t("DarkLoad has no telemetry, analytics, tracking, advertisements, accounts or cloud sync. Network requests go only to the platforms you download from, to GitHub when installing or checking components and updates, and to thumbnail servers for media you analyze.")}
        </div>
        <ToggleSetting label={t("Watch the clipboard for links")} description={t("When DarkLoad gains focus, offer to analyze a supported link you copied. Off by default.")} value={s.clipboardMonitoring} onChange={(clipboardMonitoring) => void set({ clipboardMonitoring })} />
        <ToggleSetting label={t("Desktop notifications")} description={t("Notify when downloads finish or fail.")} value={s.notifications} onChange={(notifications) => void set({ notifications })} />
      </Group>
      <Group title={t("Data")}>
        <Row label={t("Clear download history")} description={t("Removes all history entries and cached thumbnails. Files are not deleted.")}>
          <Button variant="danger" onClick={() => setConfirm("history")}>
            {t("Clear")}
          </Button>
        </Row>
        <Row label={t("Clear temporary files")} description={t("Deletes leftover partial downloads that no queued item uses.")}>
          <Button
            onClick={() =>
              api
                .clearTemporaryFiles()
                .then((freed) => toast.success(t("Temporary files cleared"), { description: t("{size} freed", { size: formatBytes(freed) }) }))
                .catch((e: { message?: string }) => toast.error(t("Could not clear temporary files"), { description: e.message }))
            }
          >
            <FileCog /> {t("Clear")}
          </Button>
        </Row>
        <Row label={t("Reset all settings")} description={t("Restores every setting to its default. History and downloads are kept.")}>
          <Button variant="danger" onClick={() => setConfirm("reset")}>
            <RotateCcw /> {t("Reset")}
          </Button>
        </Row>
      </Group>
      <ConfirmDialog
        open={confirm != null}
        onOpenChange={(o) => !o && setConfirm(null)}
        title={confirm === "history" ? t("Clear download history?") : t("Reset all settings?")}
        description={confirm === "history" ? t("Every history entry and cached thumbnail is removed. Downloaded files stay on disk. This cannot be undone.") : t("All settings return to their defaults. This cannot be undone.")}
        confirmLabel={confirm === "history" ? t("Clear history") : t("Reset settings")}
        destructive
        onConfirm={() => {
          if (confirm === "history") {
            api
              .historyClear()
              .then(() => {
                bump();
                toast.success(t("History cleared"));
              })
              .catch((e: { message?: string }) => toast.error(t("Could not clear history"), { description: e.message }));
          } else {
            api
              .settingsReset()
              .then((next) => {
                setSettings(next);
                void getCurrentWindow().setDecorations(next.nativeWindowFrame);
                toast.success(t("Settings reset"));
              })
              .catch((e: { message?: string }) => toast.error(t("Could not reset settings"), { description: e.message }));
          }
          setConfirm(null);
        }}
      />
    </>
  );
}

export function SettingsPage() {
  useT();
  const settings = useApp((s) => s.settings);
  const section = useApp((s) => s.settingsSection);
  const navigate = useApp((s) => s.navigate);
  const set = useApp((s) => s.updateSettings);
  if (!settings) return null;
  const props = { s: settings, set };
  return (
    <div className="mx-auto flex h-full max-w-[1180px] gap-8 px-8 pt-7">
      <nav className="w-48 shrink-0" aria-label={t("Settings sections")}>
        <PageHeader title={t("Settings")} />
        <ul className="-mt-2 space-y-0.5">
          {sections().map((item) => {
            const Icon = item.icon;
            return (
              <li key={item.id}>
                <button
                  type="button"
                  aria-current={section === item.id ? "page" : undefined}
                  onClick={() => navigate("settings", item.id)}
                  className={cn(
                    "flex h-9 w-full items-center gap-2.5 rounded-md px-2.5 text-[13px] font-medium transition-colors",
                    section === item.id ? "bg-elevated text-fg" : "text-fg-2 hover:bg-card hover:text-fg",
                  )}
                >
                  <Icon className={cn("size-4", section === item.id && "text-accent-hover")} />
                  {item.label}
                </button>
              </li>
            );
          })}
        </ul>
      </nav>
      <div className="min-w-0 flex-1 overflow-y-auto pt-[62px] pb-10">
        {section === "general" ? <General {...props} /> : null}
        {section === "downloads" ? <Downloads {...props} /> : null}
        {section === "appearance" ? <Appearance {...props} /> : null}
        {section === "dependencies" ? <Dependencies {...props} /> : null}
        {section === "updates" ? <Updates {...props} /> : null}
        {section === "privacy" ? <Privacy {...props} /> : null}
      </div>
    </div>
  );
}
