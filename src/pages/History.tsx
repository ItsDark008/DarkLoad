import { useCallback, useEffect, useRef, useState } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { toast } from "sonner";
import { Copy, ExternalLink, FolderOpen, History as HistoryIcon, MoreHorizontal, RotateCcw, Search, Trash2 } from "lucide-react";
import { EmptyState, PageHeader, StatusBadge } from "@/components/common";
import { PlatformIcon } from "@/components/brand";
import { Thumb } from "@/components/Thumb";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Select } from "@/components/ui/select";
import { Badge, Checkbox, Skeleton } from "@/components/ui/controls";
import { ConfirmDialog, ContextMenu, DropdownMenu, type MenuItem } from "@/components/ui/overlays";
import { formatBytes, formatDate, formatRelative } from "@/lib/format";
import { cn } from "@/lib/utils";
import { api } from "@/services/api";
import { useApp } from "@/stores/app";
import { useHistorySignal } from "@/stores/signals";
import type { HistoryEntry, HistorySort, JobState } from "@/types/api";

const PAGE_SIZE = 100;

function act(action: Promise<unknown>, failure: string, success?: string) {
  action.then(() => success && toast.success(success)).catch((e: { message?: string }) => toast.error(failure, { description: e.message }));
}

export function HistoryPage() {
  const navigate = useApp((s) => s.navigate);
  const version = useHistorySignal((s) => s.version);
  const bump = useHistorySignal((s) => s.bump);
  const [search, setSearch] = useState("");
  const [debounced, setDebounced] = useState("");
  const [platform, setPlatform] = useState("all");
  const [mediaType, setMediaType] = useState("all");
  const [status, setStatus] = useState("all");
  const [sort, setSort] = useState<HistorySort>("newest");
  const [items, setItems] = useState<HistoryEntry[]>([]);
  const [total, setTotal] = useState(0);
  const [loading, setLoading] = useState(true);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [confirm, setConfirm] = useState<null | { kind: "selected" | "clear"; ids?: string[] }>(null);
  const [deleteFiles, setDeleteFiles] = useState(false);
  const parentRef = useRef<HTMLDivElement>(null);
  const loadingMore = useRef(false);

  useEffect(() => {
    const t = window.setTimeout(() => setDebounced(search), 200);
    return () => window.clearTimeout(t);
  }, [search]);

  const query = useCallback(
    (offset: number) =>
      api.historyQuery({
        search: debounced || null,
        platform: platform === "all" ? null : platform,
        mediaType: mediaType === "all" ? null : mediaType,
        status: status === "all" ? null : status,
        sort,
        limit: PAGE_SIZE,
        offset,
      }),
    [debounced, platform, mediaType, status, sort],
  );

  useEffect(() => {
    let alive = true;
    setLoading(true);
    query(0)
      .then((page) => {
        if (!alive) return;
        setItems(page.items);
        setTotal(page.total);
        setSelected((prev) => new Set([...prev].filter((id) => page.items.some((i) => i.id === id))));
      })
      .catch((e: { message?: string }) => toast.error("Could not load history", { description: e.message }))
      .finally(() => alive && setLoading(false));
    return () => {
      alive = false;
    };
  }, [query, version]);

  const virtualizer = useVirtualizer({ count: items.length, getScrollElement: () => parentRef.current, estimateSize: () => 64, overscan: 10 });
  const virtualItems = virtualizer.getVirtualItems();
  const lastIndex = virtualItems[virtualItems.length - 1]?.index ?? 0;

  useEffect(() => {
    if (loadingMore.current || items.length >= total || lastIndex < items.length - 20) return;
    loadingMore.current = true;
    query(items.length)
      .then((page) => setItems((prev) => [...prev, ...page.items.filter((i) => !prev.some((p) => p.id === i.id))]))
      .catch(() => undefined)
      .finally(() => (loadingMore.current = false));
  }, [lastIndex, items.length, total, query]);

  const toggle = (id: string) =>
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });

  const menu = (entry: HistoryEntry): MenuItem[] => [
    { label: "Open file", icon: <ExternalLink />, disabled: !entry.fileExists, onSelect: () => act(api.historyOpen(entry.id), "Could not open the file") },
    { label: "Show in folder", icon: <FolderOpen />, disabled: !entry.fileExists, onSelect: () => act(api.historyReveal(entry.id), "Could not open the folder") },
    { label: "Copy original link", icon: <Copy />, onSelect: () => act(writeText(entry.url), "Could not copy", "Link copied") },
    {
      label: "Download again",
      icon: <RotateCcw />,
      onSelect: () =>
        api
          .historyRedownload(entry.id)
          .then(() => toast.success("Added to the queue", { action: { label: "View queue", onClick: () => navigate("queue") } }))
          .catch((e: { message?: string }) => toast.error("Could not download again", { description: e.message })),
    },
    { label: "Remove from history", icon: <Trash2 />, danger: true, separatorBefore: true, onSelect: () => setConfirm({ kind: "selected", ids: [entry.id] }) },
  ];

  const runDelete = () => {
    if (!confirm) return;
    const files = deleteFiles;
    if (confirm.kind === "clear") {
      act(
        api.historyClear().then(() => {
          setSelected(new Set());
          bump();
        }),
        "Could not clear history",
        "History cleared",
      );
    } else {
      const ids = confirm.ids ?? [...selected];
      api
        .historyDelete(ids, files)
        .then((r) => {
          setSelected(new Set());
          bump();
          toast.success(`${r.entries} ${r.entries === 1 ? "entry" : "entries"} removed`, {
            description: files ? `${r.files} files deleted${r.fileErrors ? `, ${r.fileErrors} could not be deleted` : ""}` : "Downloaded files were kept.",
          });
        })
        .catch((e: { message?: string }) => toast.error("Could not remove entries", { description: e.message }));
    }
    setConfirm(null);
    setDeleteFiles(false);
  };

  const filtersActive = debounced || platform !== "all" || mediaType !== "all" || status !== "all";

  return (
    <div className="mx-auto flex h-full max-w-[1180px] flex-col px-8 pt-7">
      <PageHeader
        title="History"
        description="Every download is recorded locally on this device. Removing an entry never deletes the file unless you ask."
        actions={
          <Button variant="ghost" disabled={total === 0 && !filtersActive} onClick={() => setConfirm({ kind: "clear" })}>
            <Trash2 /> Clear history
          </Button>
        }
      />

      <div className="mb-3 flex flex-wrap items-center gap-2">
        <div className="relative min-w-[220px] flex-1">
          <Search className="pointer-events-none absolute top-1/2 left-3 size-4 -translate-y-1/2 text-muted" />
          <Input value={search} onChange={(e) => setSearch(e.target.value)} placeholder="Search titles, creators and links" className="pl-9" aria-label="Search history" />
        </div>
        <Select aria-label="Platform" className="w-[140px]" value={platform} onValueChange={setPlatform} options={[{ value: "all", label: "All platforms" }, { value: "youtube", label: "YouTube" }, { value: "tiktok", label: "TikTok" }, { value: "instagram", label: "Instagram" }]} />
        <Select aria-label="Media type" className="w-[120px]" value={mediaType} onValueChange={setMediaType} options={[{ value: "all", label: "All types" }, { value: "video", label: "Video" }, { value: "audio", label: "Audio" }]} />
        <Select aria-label="Status" className="w-[140px]" value={status} onValueChange={setStatus} options={[{ value: "all", label: "Any status" }, { value: "completed", label: "Completed" }, { value: "failed", label: "Failed" }, { value: "cancelled", label: "Cancelled" }]} />
        <Select<HistorySort> aria-label="Sort" className="w-[140px]" value={sort} onValueChange={setSort} options={[{ value: "newest", label: "Newest first" }, { value: "oldest", label: "Oldest first" }, { value: "title", label: "Title" }, { value: "size", label: "Largest first" }]} />
      </div>

      <div className="mb-2 flex h-8 items-center justify-between text-xs text-fg-2">
        <label className="flex items-center gap-2.5">
          <Checkbox
            checked={items.length > 0 && selected.size === items.length ? true : selected.size > 0 ? "indeterminate" : false}
            onCheckedChange={(v) => setSelected(v === true ? new Set(items.map((i) => i.id)) : new Set())}
            aria-label="Select all loaded entries"
            disabled={items.length === 0}
          />
          {selected.size > 0 ? `${selected.size} selected` : `${total} ${total === 1 ? "entry" : "entries"}`}
        </label>
        {selected.size > 0 ? (
          <Button size="sm" variant="danger" onClick={() => setConfirm({ kind: "selected" })}>
            <Trash2 /> Remove selected
          </Button>
        ) : null}
      </div>

      <div ref={parentRef} className="min-h-0 flex-1 overflow-y-auto rounded-xl border border-line bg-card" role="list" aria-label="Download history">
        {loading && items.length === 0 ? (
          <div className="space-y-3 p-4">
            {Array.from({ length: 6 }, (_, i) => (
              <Skeleton key={i} className="h-10 w-full" />
            ))}
          </div>
        ) : items.length === 0 ? (
          <EmptyState
            icon={HistoryIcon}
            title={filtersActive ? "No matching downloads" : "No history yet"}
            description={filtersActive ? "Try a different search or filter." : "Completed, failed and cancelled downloads will be listed here."}
            className="h-full"
          />
        ) : (
          <div style={{ height: virtualizer.getTotalSize(), position: "relative" }}>
            {virtualItems.map((row) => {
              const entry = items[row.index];
              if (!entry) return null;
              return (
                <ContextMenu key={entry.id} items={menu(entry)}>
                  <div
                    role="listitem"
                    style={{ position: "absolute", top: 0, left: 0, width: "100%", height: row.size, transform: `translateY(${row.start}px)` }}
                    className={cn("flex items-center gap-3 border-b border-line px-3 hover:bg-elevated/60", selected.has(entry.id) && "bg-accent/[0.05]")}
                    onDoubleClick={() => entry.fileExists && act(api.historyOpen(entry.id), "Could not open the file")}
                  >
                    <Checkbox checked={selected.has(entry.id)} onCheckedChange={() => toggle(entry.id)} aria-label={`Select ${entry.title}`} />
                    <Thumb local={entry.thumbnailLocal} remote={entry.thumbnail} audio={entry.mediaType === "audio"} className="aspect-video h-10 shrink-0" />
                    <div className="min-w-0 flex-1">
                      <div className="truncate text-[13px] text-fg" title={entry.title}>
                        {entry.title}
                      </div>
                      <div className="flex min-w-0 items-center gap-1.5 text-xs text-fg-2">
                        <PlatformIcon platform={entry.platform} className="size-3" />
                        <span className="truncate">{[entry.uploader, entry.quality].filter(Boolean).join(" · ") || entry.url}</span>
                      </div>
                    </div>
                    {entry.format ? <Badge className="uppercase">{entry.format}</Badge> : null}
                    {entry.status === "completed" && !entry.fileExists ? <Badge tone="warning">File missing</Badge> : null}
                    <StatusBadge state={entry.status as JobState} />
                    <span className="tabular w-20 shrink-0 text-right text-xs text-fg-2">{formatBytes(entry.fileSize)}</span>
                    <span className="w-28 shrink-0 text-right text-xs text-fg-2" title={formatDate(entry.createdAt)}>
                      {formatRelative(entry.createdAt)}
                    </span>
                    <DropdownMenu
                      items={menu(entry)}
                      trigger={
                        <Button size="icon-sm" variant="ghost" aria-label="Entry actions">
                          <MoreHorizontal />
                        </Button>
                      }
                    />
                  </div>
                </ContextMenu>
              );
            })}
          </div>
        )}
      </div>
      <div className="pb-6" />

      <ConfirmDialog
        open={confirm != null}
        onOpenChange={(o) => {
          if (!o) {
            setConfirm(null);
            setDeleteFiles(false);
          }
        }}
        title={confirm?.kind === "clear" ? "Clear all history?" : `Remove ${confirm?.ids?.length ?? selected.size} ${(confirm?.ids?.length ?? selected.size) === 1 ? "entry" : "entries"}?`}
        description={
          confirm?.kind === "clear"
            ? "This removes every history entry and cached thumbnail. Your downloaded files are not deleted. This cannot be undone."
            : "The entries are removed from history. Files stay on disk unless you choose to delete them below."
        }
        confirmLabel={confirm?.kind === "clear" ? "Clear history" : deleteFiles ? "Remove and delete files" : "Remove"}
        destructive
        onConfirm={runDelete}
      >
        {confirm?.kind === "selected" ? (
          <label className="mt-4 flex items-center gap-2.5 text-[13px] text-fg">
            <Checkbox checked={deleteFiles} onCheckedChange={(v) => setDeleteFiles(v === true)} />
            Also permanently delete the downloaded files
          </label>
        ) : null}
      </ConfirmDialog>
    </div>
  );
}
