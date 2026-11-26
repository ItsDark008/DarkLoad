import { useId, useState, type ReactNode } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { toast } from "sonner";
import {
  Brush,
  CircleArrowUp,
  Download,
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
import { AUDIO_BITRATES, AUDIO_FORMATS, CONTAINERS } from "@/lib/options";
import { cn, isValidOledBackground } from "@/lib/utils";
import { api } from "@/services/api";
import { useApp, type SettingsSection } from "@/stores/app";
import { useHistorySignal } from "@/stores/signals";
import type { DependencyStatus, DepId, Settings } from "@/types/api";
import { OutputFolder } from "./Downloader";
import { Changelog } from "./About";

const SECTIONS: { id: SettingsSection; label: string; icon: LucideIcon }[] = [
  { id: "general", label: "General", icon: SlidersHorizontal },
  { id: "downloads", label: "Downloads", icon: Download },
  { id: "appearance", label: "Appearance", icon: Brush },
  { id: "dependencies", label: "Dependencies", icon: Package },
  { id: "updates", label: "Updates", icon: CircleArrowUp },
  { id: "privacy", label: "Privacy", icon: Shield },
];

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
      toast.error(`Enter a value between ${min} and ${max}.`);
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
      <Group title="Downloads">
        <div className="px-5 py-4">
          <div className="text-[13px] font-medium">Default download folder</div>
          <div className="mt-0.5 mb-3 text-xs text-fg-2">New downloads are saved here unless you choose another folder.</div>
          <OutputFolder value={s.downloadDir} onChange={(downloadDir) => void set({ downloadDir })} />
        </div>
        <Row label="Default media type">
          <Segmented aria-label="Default media type" size="sm" value={s.defaultMediaType} onValueChange={(defaultMediaType) => void set({ defaultMediaType })} options={[{ value: "video", label: "Video" }, { value: "audio", label: "Audio" }]} />
        </Row>
        <Row label="Default quality">
          <Select aria-label="Default quality" className="w-48" value={s.defaultQualityMode === "compatible" ? "compatible" : "best"} onValueChange={(defaultQualityMode) => void set({ defaultQualityMode })} options={[{ value: "best", label: "Best available" }, { value: "compatible", label: "Best compatible" }]} />
        </Row>
        <Row label="Default video container">
          <Select aria-label="Default container" className="w-48" value={s.defaultContainer} onValueChange={(defaultContainer) => void set({ defaultContainer })} options={CONTAINERS.map((c) => ({ value: c.value, label: c.label, description: c.description }))} />
        </Row>
        <Row label="Default audio format">
          <Select aria-label="Default audio format" className="w-48" value={s.defaultAudioFormat} onValueChange={(defaultAudioFormat) => void set({ defaultAudioFormat })} options={AUDIO_FORMATS.map((f) => ({ value: f.value, label: f.label }))} />
        </Row>
        <Row label="Default audio bitrate" description="Used when converting to a lossy format.">
          <Select aria-label="Default audio bitrate" className="w-48" value={s.defaultAudioBitrate == null ? "best" : String(s.defaultAudioBitrate)} onValueChange={(v) => void set({ defaultAudioBitrate: v === "best" ? null : Number(v) })} options={[{ value: "best", label: "Best available" }, ...AUDIO_BITRATES.map((b) => ({ value: String(b), label: `${b} kbps` }))]} />
        </Row>
        <div className="px-5 py-4">
          <div className="text-[13px] font-medium">Filename template</div>
          <div className="mt-0.5 mb-3 text-xs text-fg-2">
            yt-dlp output template for file names. Folders are not allowed; it must end with <code className="font-mono">.%(ext)s</code>.
          </div>
          <div className="flex gap-2">
            <Input className="font-mono" value={template} maxLength={240} onChange={(e) => setTemplate(e.target.value)} onKeyDown={(e) => e.key === "Enter" && saveTemplate()} aria-label="Filename template" />
            <Button disabled={template === s.filenameTemplate} onClick={saveTemplate}>
              Save
            </Button>
          </div>
        </div>
      </Group>
      <Group title="Application">
        <Row label="Language" description="DarkLoad is available in English.">
          <span className="text-[13px] text-fg-2">English</span>
        </Row>
        <ToggleSetting
          label="Use the system window frame"
          description="Replace DarkLoad's title bar with the operating system's. Useful with some Linux window managers."
          value={s.nativeWindowFrame}
          onChange={(nativeWindowFrame) => {
            void set({ nativeWindowFrame }).then(async (ok) => {
              if (ok) await getCurrentWindow().setDecorations(nativeWindowFrame);
            });
          }}
        />
        <ToggleSetting label="Confirm before closing during downloads" value={s.confirmExitWithActiveDownloads} onChange={(confirmExitWithActiveDownloads) => void set({ confirmExitWithActiveDownloads })} />
      </Group>
    </>
  );
}

function Downloads({ s, set }: { s: Settings; set: (p: Partial<Settings>) => Promise<boolean> }) {
  return (
    <>
      <Group title="Queue">
        <Row label="Concurrent downloads" description="How many downloads run at the same time.">
          <Select aria-label="Concurrent downloads" className="w-28" value={String(s.maxConcurrentDownloads)} onValueChange={(v) => void set({ maxConcurrentDownloads: Number(v) })} options={Array.from({ length: 8 }, (_, i) => ({ value: String(i + 1), label: String(i + 1) }))} />
        </Row>
        <ToggleSetting label="Resume the queue on startup" description="Unfinished downloads continue automatically when DarkLoad starts." value={s.resumeQueueOnStartup} onChange={(resumeQueueOnStartup) => void set({ resumeQueueOnStartup })} />
        <Row label="Playlists" description="Which entries are selected when a playlist is analyzed.">
          <Select aria-label="Playlist behavior" className="w-48" value={s.playlistBehavior} onValueChange={(playlistBehavior) => void set({ playlistBehavior })} options={[{ value: "all", label: "Select all entries" }, { value: "select", label: "Let me choose" }]} />
        </Row>
      </Group>
      <Group title="Network">
        <Row label="Speed limit" description="Per download. Leave empty for unlimited.">
          <NumberSetting value={s.speedLimitKib} min={16} max={10_000_000} allowEmpty placeholder="Unlimited" suffix="KiB/s" onCommit={(speedLimitKib) => void set({ speedLimitKib })} />
        </Row>
        <Row label="Retry attempts" description="Automatic retries for network interruptions and server errors.">
          <NumberSetting value={s.retryAttempts} min={0} max={10} onCommit={(v) => void set({ retryAttempts: v ?? 3 })} />
        </Row>
        <Row label="Network timeout">
          <NumberSetting value={s.socketTimeoutSecs} min={5} max={600} suffix="seconds" onCommit={(v) => void set({ socketTimeoutSecs: v ?? 30 })} />
        </Row>
        <ToggleSetting
          label="Prefer IPv4"
          description="Connect over IPv4 only. Prevents long stalls on networks whose IPv6 connectivity is broken."
          value={s.preferIpv4}
          onChange={(preferIpv4) => void set({ preferIpv4 })}
        />
        <Row label="Parallel fragments" description="Fragments fetched in parallel for segmented streams.">
          <NumberSetting value={s.concurrentFragments} min={1} max={16} onCommit={(v) => void set({ concurrentFragments: v ?? 4 })} />
        </Row>
      </Group>
      <Group title="Files">
        <Row label="When a file already exists">
          <Select aria-label="Duplicate handling" className="w-48" value={s.duplicatePolicy} onValueChange={(duplicatePolicy) => void set({ duplicatePolicy })} options={[{ value: "rename", label: "Keep both (rename)" }, { value: "skip", label: "Skip the new file" }, { value: "overwrite", label: "Overwrite" }]} />
        </Row>
        <Row label="Partial files" description="What happens to incomplete data when a download is cancelled or fails.">
          <Select aria-label="Partial files" className="w-48" value={s.partialFiles} onValueChange={(partialFiles) => void set({ partialFiles })} options={[{ value: "delete", label: "Delete" }, { value: "keep", label: "Keep to resume later" }]} />
        </Row>
      </Group>
    </>
  );
}

const OLED_BACKGROUNDS = [
  { value: "#000000", name: "Pure black" },
  { value: "#04060E", name: "Midnight" },
  { value: "#0A0510", name: "Plum" },
  { value: "#030A06", name: "Forest" },
  { value: "#0E0505", name: "Ember" },
  { value: "#0C0C0E", name: "Graphite" },
];

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
    else toast.error("Choose a darker color", { description: "The background must stay very dark so text remains readable." });
  };
  return (
    <div className="px-5 py-4">
      <div className="text-[13px] font-medium">Background</div>
      <div className="mt-0.5 text-xs text-fg-2">
        The base color of the OLED theme. Cards, borders and menus are derived from it. Pure black turns pixels off on OLED displays.
      </div>
      <div className="mt-3 flex flex-wrap items-center gap-2">
        {OLED_BACKGROUNDS.map((option) => {
          const selected = current === option.value;
          return (
            <Tooltip key={option.value} content={`${option.name} · ${option.value}`}>
              <button
                type="button"
                aria-label={`${option.name} background`}
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
          aria-label="Custom background color"
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
            Reset
          </Button>
        ) : null}
      </div>
      {invalid ? <p className="mt-2 text-xs text-danger">This color is too bright for the OLED theme.</p> : null}
    </div>
  );
}

const ACCENTS = ["#8B5CF6", "#6366F1", "#3B82F6", "#06B6D4", "#10B981", "#EAB308", "#F97316", "#EC4899"];

function Appearance({ s, set }: { s: Settings; set: (p: Partial<Settings>) => Promise<boolean> }) {
  const [hexDraft, setHex] = useState<string | null>(null);
  const hex = hexDraft ?? s.accentColor;
  return (
    <>
      <Group title="Theme">
        <Row label="Theme" description="OLED uses true-black surfaces that save power on OLED displays.">
          <Segmented aria-label="Theme" size="sm" value={s.theme} onValueChange={(theme) => void set({ theme })} options={[{ value: "oled", label: "OLED black" }, { value: "dark", label: "Dark" }]} />
        </Row>
        {s.theme === "oled" ? <OledBackgroundPicker s={s} set={set} /> : null}
        <div className="px-5 py-4">
          <div className="text-[13px] font-medium">Accent color</div>
          <div className="mt-3 flex flex-wrap items-center gap-2">
            {ACCENTS.map((c) => (
              <button
                key={c}
                type="button"
                aria-label={`Accent ${c}`}
                aria-pressed={s.accentColor.toUpperCase() === c}
                onClick={() => void set({ accentColor: c })}
                className={cn("size-7 rounded-full border-2 transition-transform hover:scale-110", s.accentColor.toUpperCase() === c ? "border-white" : "border-transparent")}
                style={{ background: c }}
              />
            ))}
            <Input
              aria-label="Custom accent color"
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
                Reset
              </Button>
            ) : null}
          </div>
        </div>
      </Group>
      <Group title="Layout">
        <Row label="Sidebar" description="Collapse automatically on narrow windows or remember your choice.">
          <Select aria-label="Sidebar behavior" className="w-48" value={s.sidebarBehavior} onValueChange={(sidebarBehavior) => void set({ sidebarBehavior })} options={[{ value: "remember", label: "Remember state" }, { value: "auto", label: "Collapse automatically" }]} />
        </Row>
        {s.sidebarBehavior === "remember" ? <ToggleSetting label="Collapse sidebar" value={s.sidebarCollapsed} onChange={(sidebarCollapsed) => void set({ sidebarCollapsed })} /> : null}
        <Row label="Interface density">
          <Segmented aria-label="Density" size="sm" value={s.density} onValueChange={(density) => void set({ density })} options={[{ value: "comfortable", label: "Comfortable" }, { value: "compact", label: "Compact" }]} />
        </Row>
        <ToggleSetting label="Animations" description="Reduced motion from your system settings is always respected." value={s.animations} onChange={(animations) => void set({ animations })} />
      </Group>
    </>
  );
}

const STAGE_LABELS: Record<string, string> = {
  resolving: "Finding latest release",
  downloading: "Downloading",
  verifying: "Verifying checksum",
  extracting: "Extracting",
  validating: "Validating",
  done: "Installed",
  failed: "Failed",
};

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
              <Badge tone="success">Available</Badge>
            ) : installing && progress?.stage === "done" ? (
              <Badge tone="success">Installed</Badge>
            ) : (
              <Badge tone={dep.required ? "danger" : "warning"}>{dep.required ? "Missing" : "Not installed"}</Badge>
            )}
            {dep.source === "system" ? <Badge>System</Badge> : dep.source === "managed" ? <Badge>Managed</Badge> : null}
            {!dep.required ? <Badge>Recommended</Badge> : null}
            {dep.updateAvailable ? <Badge tone="accent">Update: {dep.latestVersion}</Badge> : null}
          </div>
          <div className="mt-1 text-xs text-fg-2">{dep.description}</div>
          {dep.error ? <div className="mt-1 text-xs text-danger" data-selectable>{dep.error}</div> : null}
        </div>
        <div className="shrink-0 text-right">
          <div className="max-w-[260px] truncate font-mono text-[12px] text-fg" data-selectable title={dep.version ?? undefined}>
            {dep.version ?? "—"}
          </div>
          {dep.installedAt ? <div className="text-[11px] text-muted">Installed {formatDate(dep.installedAt)}</div> : null}
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
            {STAGE_LABELS[progress.stage] ?? progress.stage}
            {progress.stage === "downloading" ? ` · ${formatBytes(progress.downloaded)}${progress.total ? ` of ${formatBytes(progress.total)}` : ""}` : ""}
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
      toast.success(n ? `${n} ${n === 1 ? "update" : "updates"} available` : "All components are up to date");
    } catch (e) {
      toast.error("Could not check for updates", { description: (e as { message?: string }).message });
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
              {installing ? <Loader2 className="animate-spin" /> : <Download />} Install missing
            </Button>
          ) : null}
          {outdated.length > 0 ? (
            <Button variant="primary" disabled={installing} onClick={() => void installDeps(outdated as DepId[])}>
              <CircleArrowUp /> Update {outdated.length}
            </Button>
          ) : null}
          <Button disabled={checking || installing} onClick={() => void check()}>
            {checking ? <Loader2 className="animate-spin" /> : <RefreshCw />} Check for updates
          </Button>
          <Button variant="ghost" disabled={installing} onClick={() => setRepairOpen(true)}>
            <Wrench /> Repair
          </Button>
          <Button variant="ghost" onClick={() => void api.depsDiagnostics().then(setDiagnostics)}>
            <Stethoscope /> Diagnostics
          </Button>
        </div>
        {deps.map((d) => (
          <DependencyCard key={d.id} dep={d} />
        ))}
      </Group>
      <Group title="Sources">
        <div className="px-5 py-4 text-xs leading-relaxed text-fg-2">
          Components are downloaded only when you ask, from their official GitHub releases (yt-dlp/yt-dlp, yt-dlp/FFmpeg-Builds and denoland/deno), and installed only after their published SHA-256 checksums match. They are stored in DarkLoad&apos;s own data folder.
        </div>
        <ToggleSetting
          label="Use tools installed on this system"
          description="Fall back to yt-dlp, FFmpeg or Deno found on PATH when no managed copy exists. Off by default for predictable results."
          value={s.allowSystemBinaries}
          onChange={(allowSystemBinaries) => void set({ allowSystemBinaries })}
        />
      </Group>
      <ConfirmDialog
        open={repairOpen}
        onOpenChange={setRepairOpen}
        title="Repair components?"
        description="DarkLoad re-downloads and re-verifies yt-dlp, FFmpeg and Deno from their official releases, replacing the current copies."
        confirmLabel="Repair"
        onConfirm={() => void installDeps(["yt-dlp", "ffmpeg", "deno"], true)}
      />
      <Dialog open={diagnostics != null} onOpenChange={(o) => !o && setDiagnostics(null)}>
        <DialogContent
          title="Diagnostics"
          description="Share this report when asking for help. It contains no personal data beyond file paths."
          className="w-[min(760px,calc(100vw-48px))]"
          footer={
            <Button variant="primary" onClick={() => diagnostics && void writeText(diagnostics).then(() => toast.success("Diagnostics copied"))}>
              Copy report
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
      const result = await api.checkAppUpdate();
      setUpdate(result);
      setMessage(result.available ? null : `DarkLoad ${result.currentVersion} is the latest version.`);
    } catch (e) {
      setMessage((e as { message?: string }).message ?? "Could not check for updates.");
    } finally {
      setChecking(false);
    }
  };

  return (
    <>
      <Group title="Application">
        <Row label="Current version">
          <span className="tabular font-mono text-[13px]">{info?.version}</span>
        </Row>
        <ToggleSetting label="Check for updates automatically" description="Checks once at startup. Only the version information is requested; nothing about you is sent." value={s.autoCheckUpdates} onChange={(autoCheckUpdates) => void set({ autoCheckUpdates })} />
        <div className="px-5 py-4">
          <div className="flex flex-wrap items-center gap-2">
            <Button disabled={checking} onClick={() => void check()}>
              {checking ? <Loader2 className="animate-spin" /> : <RefreshCw />} Check now
            </Button>
            {update?.available ? (
              <Button
                variant="primary"
                disabled={installing}
                onClick={() => {
                  setInstalling(true);
                  api
                    .installAppUpdate()
                    .catch((e: { message?: string }) => toast.error("Update failed", { description: e.message }))
                    .finally(() => setInstalling(false));
                }}
              >
                {installing ? <Loader2 className="animate-spin" /> : <CircleArrowUp />} Install {update.version} and restart
              </Button>
            ) : null}
          </div>
          {message ? <p className="mt-3 text-xs text-fg-2">{message}</p> : null}
          {!info?.updaterConfigured ? <p className="mt-3 text-xs text-fg-2">This build was compiled without an update channel. New versions are available from the project&apos;s releases page.</p> : null}
          {update?.available && update.notes ? (
            <pre className="mt-4 max-h-56 overflow-auto rounded-lg border border-line bg-bg p-3 font-sans text-xs leading-relaxed whitespace-pre-wrap text-fg-2" data-selectable>
              {update.notes}
            </pre>
          ) : null}
        </div>
      </Group>
      <Group title="Release changelog">
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
      <Group title="Privacy">
        <div className="flex items-start gap-3 px-5 py-4 text-xs leading-relaxed text-fg-2">
          <Shield className="mt-0.5 size-4 shrink-0 text-success" />
          DarkLoad has no telemetry, analytics, tracking, advertisements, accounts or cloud sync. Network requests go only to the platforms you download from, to GitHub when installing or checking components and updates, and to thumbnail servers for media you analyze.
        </div>
        <ToggleSetting label="Watch the clipboard for links" description="When DarkLoad gains focus, offer to analyze a supported link you copied. Off by default." value={s.clipboardMonitoring} onChange={(clipboardMonitoring) => void set({ clipboardMonitoring })} />
        <ToggleSetting label="Desktop notifications" description="Notify when downloads finish or fail." value={s.notifications} onChange={(notifications) => void set({ notifications })} />
      </Group>
      <Group title="Data">
        <Row label="Clear download history" description="Removes all history entries and cached thumbnails. Files are not deleted.">
          <Button variant="danger" onClick={() => setConfirm("history")}>
            Clear
          </Button>
        </Row>
        <Row label="Clear temporary files" description="Deletes leftover partial downloads that no queued item uses.">
          <Button
            onClick={() =>
              api
                .clearTemporaryFiles()
                .then((freed) => toast.success("Temporary files cleared", { description: `${formatBytes(freed)} freed` }))
                .catch((e: { message?: string }) => toast.error("Could not clear temporary files", { description: e.message }))
            }
          >
            <FileCog /> Clear
          </Button>
        </Row>
        <Row label="Reset all settings" description="Restores every setting to its default. History and downloads are kept.">
          <Button variant="danger" onClick={() => setConfirm("reset")}>
            <RotateCcw /> Reset
          </Button>
        </Row>
      </Group>
      <ConfirmDialog
        open={confirm != null}
        onOpenChange={(o) => !o && setConfirm(null)}
        title={confirm === "history" ? "Clear download history?" : "Reset all settings?"}
        description={confirm === "history" ? "Every history entry and cached thumbnail is removed. Downloaded files stay on disk. This cannot be undone." : "All settings return to their defaults. This cannot be undone."}
        confirmLabel={confirm === "history" ? "Clear history" : "Reset settings"}
        destructive
        onConfirm={() => {
          if (confirm === "history") {
            api
              .historyClear()
              .then(() => {
                bump();
                toast.success("History cleared");
              })
              .catch((e: { message?: string }) => toast.error("Could not clear history", { description: e.message }));
          } else {
            api
              .settingsReset()
              .then((next) => {
                setSettings(next);
                void getCurrentWindow().setDecorations(next.nativeWindowFrame);
                toast.success("Settings reset");
              })
              .catch((e: { message?: string }) => toast.error("Could not reset settings", { description: e.message }));
          }
          setConfirm(null);
        }}
      />
    </>
  );
}

export function SettingsPage() {
  const settings = useApp((s) => s.settings);
  const section = useApp((s) => s.settingsSection);
  const navigate = useApp((s) => s.navigate);
  const set = useApp((s) => s.updateSettings);
  if (!settings) return null;
  const props = { s: settings, set };
  return (
    <div className="mx-auto flex h-full max-w-[1180px] gap-8 px-8 pt-7">
      <nav className="w-48 shrink-0" aria-label="Settings sections">
        <PageHeader title="Settings" />
        <ul className="-mt-2 space-y-0.5">
          {SECTIONS.map((item) => {
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
