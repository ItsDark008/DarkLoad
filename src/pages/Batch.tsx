import { useEffect, useMemo, useRef, useState } from "react";
import { toast } from "sonner";
import { AlertCircle, CheckCircle2, FileText, Layers, ListChecks, Loader2, MoreHorizontal, Plus, Search, Settings2, Trash2, Upload } from "lucide-react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { DownloadOptionsForm } from "@/components/DownloadOptionsForm";
import { DownloadPresets } from "@/components/DownloadPresets";
import { EmptyState, PageHeader } from "@/components/common";
import { PlaylistPicker } from "@/components/media";
import { PlatformIcon } from "@/components/brand";
import { Thumb } from "@/components/Thumb";
import { Button } from "@/components/ui/button";
import { Textarea } from "@/components/ui/input";
import { Badge, Card, Checkbox, SectionTitle, Tooltip } from "@/components/ui/controls";
import { Dialog, DialogContent, DropdownMenu } from "@/components/ui/overlays";
import { formatDuration } from "@/lib/format";
import { optionsForPlatform, optionsFromSettings } from "@/lib/options";
import { dedupeUrls, extractUrls } from "@/lib/urls";
import { cn } from "@/lib/utils";
import { api } from "@/services/api";
import { useApp } from "@/stores/app";
import { MAX_BATCH_ITEMS, useBatch, type BatchItem } from "@/stores/batch";
import type { DownloadRequest } from "@/types/api";
import { OutputFolder } from "./Downloader";

const MAX_FILE_BYTES = 2 * 1024 * 1024;

function StatusCell({ item }: { item: BatchItem }) {
  switch (item.status) {
    case "pending":
      return <Badge>Not analyzed</Badge>;
    case "analyzing":
      return (
        <Badge tone="accent">
          <Loader2 className="animate-spin" /> Analyzing
        </Badge>
      );
    case "ready":
      return (
        <Badge tone="success">
          <CheckCircle2 /> {item.analysis?.kind === "playlist" ? `${item.entries.length}/${item.analysis.entries.length} items` : "Ready"}
        </Badge>
      );
    case "queued":
      return <Badge tone="accent">Added to queue</Badge>;
    case "error":
      return (
        <Tooltip content={item.error?.message ?? "Failed"}>
          <Badge tone="danger">
            <AlertCircle /> Error
          </Badge>
        </Tooltip>
      );
  }
}

export function BatchPage() {
  const settings = useApp((s) => s.settings);
  const navigate = useApp((s) => s.navigate);
  const { items, globalOptions, setGlobalOptions, outputDir, setOutputDir, addUrls, remove, update, setAllSelected, clear, analyzeAll, analyzeOne, analyzing } = useBatch();
  const [text, setText] = useState("");
  const [dragging, setDragging] = useState(false);
  const [editing, setEditing] = useState<BatchItem | null>(null);
  const [submitting, setSubmitting] = useState(false);
  const fileInput = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLDivElement>(null);
  const folder = outputDir ?? settings?.downloadDir ?? "";

  useEffect(() => {
    if (!globalOptions && settings) setGlobalOptions(optionsFromSettings(settings));
  }, [globalOptions, settings, setGlobalOptions]);

  const virtualizer = useVirtualizer({ count: items.length, getScrollElement: () => listRef.current, estimateSize: () => 58, overscan: 10 });

  const ingest = (raw: string, source: string) => {
    const urls = extractUrls(raw);
    if (urls.length === 0) {
      toast.error("No supported links found", { description: `${source} did not contain YouTube, TikTok, Instagram or Spotify links.` });
      return;
    }
    const { added, duplicates } = addUrls(urls);
    const skipped = urls.length - added - duplicates;
    toast.success(`${added} ${added === 1 ? "link" : "links"} added`, {
      description: [duplicates ? `${duplicates} duplicates removed` : "", skipped > 0 ? `${skipped} over the ${MAX_BATCH_ITEMS} item limit` : ""].filter(Boolean).join(" · ") || undefined,
    });
  };

  const readFile = async (file: File) => {
    if (file.size > MAX_FILE_BYTES) {
      toast.error("File too large", { description: "Link lists can be up to 2 MB." });
      return;
    }
    if (file.type && !file.type.startsWith("text/") && !/\.(txt|csv|md|list)$/i.test(file.name)) {
      toast.error("Unsupported file", { description: "Import a plain-text file with one link per line." });
      return;
    }
    ingest(await file.text(), file.name);
  };

  const readyItems = items.filter((i) => i.status === "ready" && i.selected);
  const requestCount = readyItems.reduce((n, i) => n + (i.analysis?.kind === "playlist" ? i.entries.length : 1), 0);
  const allReadySelected = items.some((i) => i.status === "ready") && items.filter((i) => i.status === "ready").every((i) => i.selected);
  const counts = useMemo(
    () => ({
      pending: items.filter((i) => i.status === "pending").length,
      errors: items.filter((i) => i.status === "error").length,
      queued: items.filter((i) => i.status === "queued").length,
    }),
    [items],
  );

  const addToQueue = async () => {
    if (!globalOptions) return;
    const requests: DownloadRequest[] = [];
    for (const item of readyItems) {
      const a = item.analysis;
      if (!a) continue;
      const options = optionsForPlatform(item.options ?? globalOptions, a.platform);
      if (a.kind === "media") {
        requests.push({ url: a.url, playlistItem: a.playlistItem, options, outputDir, title: a.title, uploader: a.uploader, thumbnail: a.thumbnail, thumbnailLocal: a.thumbnailLocal, duration: a.duration, spotify: a.spotify ?? null });
      } else {
        for (const entry of a.entries.filter((e) => item.entries.includes(e.index) && e.available)) {
          requests.push(
            entry.media
              ? { url: entry.media.url, playlistItem: entry.media.playlistItem, options, outputDir, title: entry.title, thumbnail: entry.thumbnail, duration: entry.duration }
              : { url: entry.url, options, outputDir, title: entry.title, uploader: entry.uploader, thumbnail: entry.thumbnail, duration: entry.duration, spotify: entry.spotify ?? null },
          );
        }
      }
    }
    if (requests.length === 0) return;
    setSubmitting(true);
    try {
      const jobs = await api.enqueue(requests);
      readyItems.forEach((i) => update(i.id, { status: "queued", selected: false }));
      toast.success(`${jobs.length} downloads added to the queue`, { action: { label: "View queue", onClick: () => navigate("queue") } });
    } catch (e) {
      toast.error("Could not add downloads", { description: (e as { message?: string }).message });
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <div
      className="mx-auto flex h-full max-w-[1180px] flex-col px-8 pt-7"
      onDragOver={(e) => {
        e.preventDefault();
        setDragging(true);
      }}
      onDragLeave={(e) => {
        if (e.currentTarget === e.target) setDragging(false);
      }}
      onDrop={(e) => {
        e.preventDefault();
        setDragging(false);
        const file = e.dataTransfer.files[0];
        if (file) void readFile(file);
        else ingest(e.dataTransfer.getData("text/uri-list") || e.dataTransfer.getData("text/plain"), "The dropped content");
      }}
    >
      <PageHeader
        title="Batch Download"
        description="Paste, import or drop many links, analyze them together and queue everything at once."
        actions={
          <>
            <Button variant="ghost" onClick={() => fileInput.current?.click()}>
              <Upload /> Import file
            </Button>
            <input
              ref={fileInput}
              type="file"
              accept=".txt,.csv,.md,.list,text/plain"
              className="hidden"
              onChange={(e) => {
                const file = e.target.files?.[0];
                if (file) void readFile(file);
                e.target.value = "";
              }}
            />
          </>
        }
      />

      <div className="grid min-h-0 flex-1 grid-cols-1 gap-6 lg:grid-cols-[minmax(0,1fr)_340px]">
        <div className="flex min-h-0 min-w-0 flex-col gap-4">
          <Card className={cn("p-4 transition-colors", dragging && "border-accent bg-accent/[0.04]")}>
            <Textarea
              rows={4}
              value={text}
              onChange={(e) => setText(e.target.value)}
              placeholder={"One link per line, or paste any text containing links.\nYou can also drop a .txt file anywhere on this page."}
              aria-label="Links to add"
              onKeyDown={(e) => {
                if ((e.ctrlKey || e.metaKey) && e.key === "Enter") {
                  ingest(text, "The text");
                  setText("");
                }
              }}
            />
            <div className="mt-3 flex flex-wrap items-center justify-between gap-2">
              <span className="text-xs text-fg-2">{extractUrls(text).length} links detected</span>
              <div className="flex gap-2">
                <Button
                  size="sm"
                  variant="ghost"
                  disabled={!text.trim()}
                  onClick={() => {
                    const urls = extractUrls(text);
                    const { unique, duplicates } = dedupeUrls(urls);
                    setText(unique.join("\n"));
                    toast.message(duplicates ? `${duplicates} duplicate links removed` : "No duplicates found");
                  }}
                >
                  Remove duplicates
                </Button>
                <Button
                  size="sm"
                  variant="primary"
                  disabled={!text.trim()}
                  onClick={() => {
                    ingest(text, "The text");
                    setText("");
                  }}
                >
                  <Plus /> Add links
                </Button>
              </div>
            </div>
          </Card>

          <div className="flex flex-wrap items-center justify-between gap-2">
            <label className="flex items-center gap-2.5 text-[13px] text-fg-2">
              <Checkbox checked={allReadySelected} onCheckedChange={(v) => setAllSelected(v === true)} aria-label="Select all ready items" disabled={!items.some((i) => i.status === "ready")} />
              {items.length} {items.length === 1 ? "item" : "items"}
            </label>
            <div className="flex flex-wrap gap-2">
              <Button size="sm" variant="primary" disabled={analyzing || (counts.pending === 0 && counts.errors === 0)} onClick={() => void analyzeAll()}>
                {analyzing ? <Loader2 className="animate-spin" /> : <Search />} Analyze {counts.pending + counts.errors || ""}
              </Button>
              <DropdownMenu
                trigger={
                  <Button size="sm" variant="ghost" aria-label="Clear items" disabled={items.length === 0}>
                    <Trash2 /> Clear
                  </Button>
                }
                items={[
                  { label: `Clear invalid (${counts.errors})`, onSelect: () => clear("invalid"), disabled: counts.errors === 0 },
                  { label: `Clear added to queue (${counts.queued})`, onSelect: () => clear("queued"), disabled: counts.queued === 0 },
                  { label: "Clear all", onSelect: () => clear("all"), danger: true, separatorBefore: true },
                ]}
              />
            </div>
          </div>

          <div ref={listRef} className="min-h-0 flex-1 overflow-y-auto rounded-xl border border-line bg-card">
            {items.length === 0 ? (
              <EmptyState icon={Layers} title="No links yet" description="Add links above, import a text file, or drop a list onto this page." className="h-full" />
            ) : (
              <div style={{ height: virtualizer.getTotalSize(), position: "relative" }}>
                {virtualizer.getVirtualItems().map((row) => {
                  const item = items[row.index];
                  if (!item) return null;
                  const a = item.analysis;
                  const title = a ? a.title : item.url;
                  return (
                    <div
                      key={item.id}
                      style={{ position: "absolute", top: 0, left: 0, width: "100%", height: row.size, transform: `translateY(${row.start}px)` }}
                      className="flex items-center gap-3 border-b border-line px-3"
                    >
                      <Checkbox checked={item.selected} disabled={item.status !== "ready"} onCheckedChange={(v) => update(item.id, { selected: v === true })} aria-label={`Select ${title}`} />
                      {a ? (
                        <Thumb local={a.kind === "media" ? a.thumbnailLocal : null} remote={a.thumbnail} className="aspect-video h-9 shrink-0" />
                      ) : (
                        <div className="flex aspect-video h-9 shrink-0 items-center justify-center rounded-md border border-line bg-elevated">
                          <PlatformIcon platform={item.platform} className="text-fg-2" />
                        </div>
                      )}
                      <div className="min-w-0 flex-1">
                        <div className="truncate text-[13px] text-fg" title={title}>
                          {title}
                        </div>
                        <div className="truncate text-xs text-fg-2">
                          {item.status === "error" ? (
                            <span className="text-danger">{item.error?.message}</span>
                          ) : a?.kind === "media" ? (
                            [a.uploader, formatDuration(a.duration), a.summary.maxHeight ? `up to ${a.summary.maxHeight}p` : null, item.options ? "custom settings" : null].filter(Boolean).join(" · ")
                          ) : a?.kind === "playlist" ? (
                            `Playlist · ${a.entries.length} items${item.options ? " · custom settings" : ""}`
                          ) : (
                            item.url
                          )}
                        </div>
                      </div>
                      <StatusCell item={item} />
                      <DropdownMenu
                        trigger={
                          <Button size="icon-sm" variant="ghost" aria-label="Item actions">
                            <MoreHorizontal />
                          </Button>
                        }
                        items={[
                          { label: a?.kind === "playlist" ? "Choose items & settings" : "Customize settings", icon: <Settings2 />, onSelect: () => setEditing(item), disabled: item.status !== "ready" },
                          { label: "Analyze again", icon: <Search />, onSelect: () => void analyzeOne(item.id), disabled: item.status === "analyzing" },
                          ...(item.options ? [{ label: "Use batch settings", icon: <ListChecks />, onSelect: () => update(item.id, { options: null }) }] : []),
                          { label: "Remove", icon: <Trash2 />, danger: true, separatorBefore: true, onSelect: () => remove([item.id]) },
                        ]}
                      />
                    </div>
                  );
                })}
              </div>
            )}
          </div>
          <div className="pb-6" />
        </div>

        <aside className="min-w-0 space-y-4 overflow-y-auto pb-6">
          <Card className="p-5">
            <SectionTitle>Batch settings</SectionTitle>
            <p className="mb-4 text-xs text-fg-2">Applied to every item unless you customize it. Each item uses the best formats it actually offers.</p>
            {globalOptions ? (
              <>
                <DownloadPresets page="batch" options={globalOptions} outputDir={folder} onApply={(preset) => {
                  setGlobalOptions(preset.options);
                  setOutputDir(preset.outputDir);
                }} />
                <DownloadOptionsForm options={globalOptions} onChange={(patch) => setGlobalOptions({ ...globalOptions, ...patch })} />
              </>
            ) : null}
          </Card>
          <Card className="space-y-4 p-5">
            <div>
              <div className="mb-1.5 text-xs font-medium text-fg-2">Save to</div>
              <OutputFolder value={folder} onChange={setOutputDir} />
            </div>
            <Button variant="primary" size="lg" className="w-full" disabled={requestCount === 0 || submitting} onClick={() => void addToQueue()}>
              {submitting ? <Loader2 className="animate-spin" /> : <FileText />}
              Add {requestCount || ""} to queue
            </Button>
          </Card>
        </aside>
      </div>

      <Dialog open={editing != null} onOpenChange={(o) => !o && setEditing(null)}>
        {editing && globalOptions ? (
          <BatchItemEditor
            item={editing}
            fallback={globalOptions}
            onSave={(patch) => {
              update(editing.id, patch);
              setEditing(null);
            }}
          />
        ) : null}
      </Dialog>
    </div>
  );
}

function BatchItemEditor({ item, fallback, onSave }: { item: BatchItem; fallback: NonNullable<BatchItem["options"]>; onSave: (patch: Partial<BatchItem>) => void }) {
  const [options, setOptions] = useState(() => optionsForPlatform(item.options ?? fallback, item.analysis?.platform ?? item.platform ?? undefined));
  const [entries, setEntries] = useState(new Set(item.entries));
  const a = item.analysis;
  return (
    <DialogContent
      title={a?.title ?? item.url}
      description={a?.kind === "playlist" ? "Choose which items to download and how." : "Settings for this item only."}
      className="w-[min(760px,calc(100vw-48px))]"
      footer={
        <Button variant="primary" onClick={() => onSave({ options, entries: [...entries], selected: a?.kind === "playlist" ? entries.size > 0 : true })}>
          Save
        </Button>
      }
    >
      <div className="space-y-5">
        {a?.kind === "playlist" ? <PlaylistPicker playlist={a} selected={entries} onSelectedChange={setEntries} height={280} /> : null}
        <DownloadOptionsForm options={options} onChange={(patch) => setOptions((o) => ({ ...o, ...patch }))} summary={a?.kind === "media" ? a.summary : null} formats={a?.kind === "media" ? a.formats : undefined} showGlobal={false} audioOnly={a?.platform === "spotify" || item.platform === "spotify"} />
      </div>
    </DialogContent>
  );
}
