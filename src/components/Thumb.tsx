import { useEffect, useRef, useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { Film, Music } from "lucide-react";
import { api } from "@/services/api";
import { cn } from "@/lib/utils";

// Remote thumbnail URL → local cached file path (or null when unavailable).
const cache = new Map<string, Promise<string | null>>();

function resolveLocal(remote: string): Promise<string | null> {
  let pending = cache.get(remote);
  if (!pending) {
    pending = api.cacheThumbnail(remote).catch(() => null);
    cache.set(remote, pending);
  }
  return pending;
}

function toSrc(path: string): string {
  try {
    return convertFileSrc(path);
  } catch {
    return "";
  }
}

interface ThumbProps {
  local?: string | null;
  remote?: string | null;
  audio?: boolean;
  className?: string;
  alt?: string;
}

/**
 * Thumbnails are always shown from DarkLoad's local cache. Remote images are fetched by the
 * backend once the element scrolls into view, which keeps the UI free of third-party requests.
 */
export function Thumb({ local, remote, audio, className, alt = "" }: ThumbProps) {
  const ref = useRef<HTMLDivElement>(null);
  const [resolved, setResolved] = useState<{ key: string; src: string | null } | null>(null);
  const [failed, setFailed] = useState(false);
  const key = `${local ?? ""}|${remote ?? ""}`;
  const src = local ? toSrc(local) : resolved?.key === key ? resolved.src : null;

  useEffect(() => {
    if (local || !remote) return;
    const el = ref.current;
    if (!el) return;
    let cancelled = false;
    const observer = new IntersectionObserver(
      (entries) => {
        if (entries.some((e) => e.isIntersecting)) {
          observer.disconnect();
          void resolveLocal(remote).then((path) => {
            if (!cancelled) setResolved({ key, src: path ? toSrc(path) : null });
          });
        }
      },
      { rootMargin: "200px" },
    );
    observer.observe(el);
    return () => {
      cancelled = true;
      observer.disconnect();
    };
  }, [local, remote, key]);

  const Fallback = audio ? Music : Film;
  return (
    <div ref={ref} className={cn("relative overflow-hidden rounded-md border border-line bg-elevated", className)}>
      {src && !failed ? (
        <img src={src} alt={alt} loading="lazy" decoding="async" className="size-full object-cover" onError={() => setFailed(true)} draggable={false} />
      ) : (
        <div className="flex size-full items-center justify-center text-muted">
          <Fallback className="size-1/3 max-h-6 max-w-6" strokeWidth={1.5} />
        </div>
      )}
    </div>
  );
}
