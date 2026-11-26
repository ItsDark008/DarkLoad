// Client-side URL helpers used for instant feedback (platform icons, batch parsing).
// The Rust backend performs the authoritative validation before anything is downloaded.
import type { Platform } from "@/types/api";

const URL_PATTERN = /\b(?:https?:\/\/)?(?:[a-z0-9-]+\.)*(?:youtube\.com|youtu\.be|youtube-nocookie\.com|tiktok\.com|instagram\.com|instagr\.am|open\.spotify\.com|spotify\.link)\/[^\s<>"'`]*|\bspotify:(?:track|album|playlist):[A-Za-z0-9]{22}\b/gi;
const MAX_INPUT = 2048;

function hostOf(input: string): string | null {
  const trimmed = input.trim();
  if (!trimmed || trimmed.length > MAX_INPUT || /\s/.test(trimmed)) return null;
  try {
    const url = new URL(trimmed.includes("://") ? trimmed : `https://${trimmed}`);
    if (url.protocol !== "https:" && url.protocol !== "http:") return null;
    return url.hostname.toLowerCase().replace(/\.$/, "");
  } catch {
    return null;
  }
}

function matchesDomain(host: string, domain: string): boolean {
  return host === domain || host.endsWith(`.${domain}`);
}

/** Detects the platform of a URL by host name only. */
export function detectPlatform(input: string): Platform | null {
  if (/^spotify:/i.test(input.trim())) return "spotify";
  const host = hostOf(input);
  if (!host) return null;
  if (host === "youtu.be" || matchesDomain(host, "youtube.com") || matchesDomain(host, "youtube-nocookie.com")) return "youtube";
  if (matchesDomain(host, "tiktok.com")) return "tiktok";
  if (matchesDomain(host, "instagram.com") || host === "instagr.am") return "instagram";
  if (host === "open.spotify.com" || host === "play.spotify.com" || host === "spotify.link" || host === "spotify.app.link") return "spotify";
  return null;
}

/** Strips trailing punctuation that commonly sticks to links pasted from prose. */
function cleanMatch(match: string): string {
  return match.replace(/[),.;:!?\]]+$/, "");
}

/** Extracts supported links from arbitrary text (pasted lists, files, drag and drop). */
export function extractUrls(text: string): string[] {
  const found = text.match(URL_PATTERN) ?? [];
  return found.map(cleanMatch).map((u) => (u.includes("://") || /^spotify:/i.test(u) ? u : `https://${u}`));
}

/** Canonical key used to detect duplicate links regardless of tracking parameters. */
export function urlKey(input: string): string {
  const uri = input.trim().match(/^spotify:(track|album|playlist):([A-Za-z0-9]{22})$/i);
  if (uri) return `spotify:${uri[1]!.toLowerCase()}:${uri[2]}`;
  try {
    const url = new URL(input.includes("://") ? input : `https://${input}`);
    const host = url.hostname.toLowerCase().replace(/^(www|m)\./, "");
    if (host === "youtu.be") return `youtube:${url.pathname.slice(1)}`;
    if (host.endsWith("youtube.com")) {
      const v = url.searchParams.get("v");
      if (v) return `youtube:${v}`;
      const list = url.searchParams.get("list");
      const shorts = url.pathname.match(/^\/(?:shorts|live|embed)\/([^/]+)/);
      if (shorts?.[1]) return `youtube:${shorts[1]}`;
      if (list) return `youtube-list:${list}`;
    }
    if (host === "open.spotify.com") {
      const entity = url.pathname.match(/^\/(?:intl-[a-z-]+\/|embed\/)?(track|album|playlist)\/([A-Za-z0-9]{22})/i);
      if (entity) return `spotify:${entity[1]!.toLowerCase()}:${entity[2]}`;
    }
    return `${host}${url.pathname.replace(/\/+$/, "")}`.toLowerCase();
  } catch {
    return input.trim().toLowerCase();
  }
}

/** Removes duplicate links, keeping the first occurrence. */
export function dedupeUrls(urls: string[]): { unique: string[]; duplicates: number } {
  const seen = new Set<string>();
  const unique: string[] = [];
  for (const url of urls) {
    const key = urlKey(url);
    if (seen.has(key)) continue;
    seen.add(key);
    unique.push(url);
  }
  return { unique, duplicates: urls.length - unique.length };
}

export const PLATFORM_NAMES: Record<Platform, string> = {
  youtube: "YouTube",
  tiktok: "TikTok",
  instagram: "Instagram",
  spotify: "Spotify",
};
