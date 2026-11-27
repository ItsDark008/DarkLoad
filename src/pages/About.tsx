import { Fragment, type ReactNode } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { ExternalLink, Keyboard, Music, Scale, Shield } from "lucide-react";
import { LogoMark, PlatformIcon } from "@/components/brand";
import { Button } from "@/components/ui/button";
import { Card, Kbd, SectionTitle } from "@/components/ui/controls";
import { modKey } from "@/lib/utils";
import { useApp } from "@/stores/app";

/** Renders the subset of Markdown used by CHANGELOG.md (headings, bullets, links as text). */
export function Changelog({ markdown }: { markdown: string }) {
  const blocks: ReactNode[] = [];
  let list: string[] = [];
  const flush = (key: number) => {
    if (list.length) {
      blocks.push(
        <ul key={`l${key}`} className="mb-3 list-disc space-y-1 pl-5 text-[13px] leading-relaxed text-fg-2">
          {list.map((item, i) => (
            <li key={i}>{item}</li>
          ))}
        </ul>,
      );
      list = [];
    }
  };
  const clean = (text: string) => text.replace(/\[([^\]]+)\]\([^)]+\)/g, "$1").replace(/[*`]/g, "");
  markdown.split(/\r?\n/).forEach((line, i) => {
    if (line.startsWith("- ")) {
      list.push(clean(line.slice(2)));
      return;
    }
    flush(i);
    if (line.startsWith("# ")) return;
    if (line.startsWith("## ")) blocks.push(<h3 key={i} className="mt-4 mb-2 text-[14px] font-semibold text-fg first:mt-0">{clean(line.slice(3))}</h3>);
    else if (line.startsWith("### ")) blocks.push(<h4 key={i} className="mt-2 mb-1.5 text-xs font-semibold tracking-wide text-fg-2 uppercase">{clean(line.slice(4))}</h4>);
    else if (line.trim()) blocks.push(<p key={i} className="mb-2 text-[13px] text-fg-2">{clean(line)}</p>);
  });
  flush(-1);
  return <div data-selectable>{blocks.map((b, i) => <Fragment key={i}>{b}</Fragment>)}</div>;
}

const SHORTCUTS: [string[], string][] = [
  [[modKey, "1…8"], "Switch between pages"],
  [[modKey, "L"], "Focus the link field"],
  [[modKey, "Shift", "V"], "Paste a link and analyze it"],
  [[modKey, "Enter"], "Start the download"],
  [[modKey, "B"], "Collapse or expand the sidebar"],
  [[modKey, ","], "Open settings"],
  [["Alt", "↑ / ↓"], "Reorder the focused queue item"],
  [["Delete"], "Cancel or remove the focused queue item"],
  [["?"], "Show keyboard shortcuts"],
];

export function ShortcutList() {
  return (
    <ul className="divide-y divide-line">
      {SHORTCUTS.map(([keys, label]) => (
        <li key={label} className="flex items-center justify-between gap-4 py-2 text-[13px]">
          <span className="text-fg-2">{label}</span>
          <span className="flex gap-1">
            {keys.map((k) => (
              <Kbd key={k}>{k}</Kbd>
            ))}
          </span>
        </li>
      ))}
    </ul>
  );
}

const NOTICES = [
  ["Tauri", "MIT / Apache-2.0", "Desktop application framework"],
  ["React", "MIT", "User interface library"],
  ["Radix UI & shadcn/ui patterns", "MIT", "Accessible interface primitives"],
  ["Lucide", "ISC", "Icons"],
  ["Inter", "SIL Open Font License 1.1", "Typeface"],
  ["SQLite (via rusqlite)", "Public domain / MIT", "Local database"],
  ["yt-dlp", "The Unlicense", "Media extraction (downloaded on request)"],
  ["FFmpeg (yt-dlp/FFmpeg-Builds)", "GPL-3.0", "Media processing (downloaded on request)"],
  ["Deno", "MIT", "JavaScript runtime used by yt-dlp (downloaded on request)"],
];

export function AboutPage() {
  const info = useApp((s) => s.info);
  return (
    <div className="mx-auto max-w-[880px] overflow-y-auto px-8 py-10" style={{ maxHeight: "100%" }}>
      <div className="flex items-center gap-5">
        <div className="flex size-20 items-center justify-center rounded-[22px] border border-line bg-card">
          <LogoMark className="size-11 text-fg" />
        </div>
        <div>
          <h1 className="text-[24px] font-semibold tracking-tight">DarkLoad</h1>
          <p className="text-[14px] text-fg-2">One link. Maximum quality.</p>
          <p className="tabular mt-1 text-xs text-muted">
            Version {info?.version} · Tauri {info?.tauriVersion} · {info?.os} {info?.arch}
          </p>
          <p className="mt-0.5 text-xs text-fg-2">
            Created by <span className="font-medium text-fg">ItsDark008</span>
          </p>
        </div>
      </div>

      <p className="mt-6 max-w-2xl text-[13px] leading-relaxed text-fg-2">
        DarkLoad is a free, open-source desktop downloader for public YouTube, TikTok and Instagram media and Spotify songs. It inspects what each source actually offers and keeps the original streams whenever possible, without re-encoding or inventing quality that is not there.
      </p>
      {info?.repository ? (
        <Button className="mt-4" variant="secondary" onClick={() => void openUrl(`https://github.com/${info.repository}`)}>
          <ExternalLink /> Source code
        </Button>
      ) : null}

      <div className="mt-8 grid gap-4 md:grid-cols-2">
        <Card className="p-5">
          <SectionTitle>
            <span className="flex items-center gap-2">
              <Shield className="size-4 text-success" /> Responsible use
            </span>
          </SectionTitle>
          <p className="text-[13px] leading-relaxed text-fg-2">
            Download only content you own or are authorized to save. DarkLoad does not bypass DRM, paywalls, private content or other access controls, and it does not sign in to any platform.
          </p>
        </Card>
        <Card className="p-5">
          <SectionTitle>
            <span className="flex items-center gap-2">
              <Keyboard className="size-4" /> Keyboard shortcuts
            </span>
          </SectionTitle>
          <ShortcutList />
        </Card>
      </div>

      <Card className="mt-4 p-5">
        <SectionTitle>
          <span className="flex items-center gap-2">
            <PlatformIcon platform="spotify" className="size-4" /> Spotify downloads
          </span>
        </SectionTitle>
        <div className="space-y-2 text-[13px] leading-relaxed text-fg-2">
          <p>
            Spotify's audio is protected by DRM, so DarkLoad never downloads it. For a Spotify song it reads the title, artists, album, duration and cover from Spotify, finds the same recording on YouTube by title, artist and duration, downloads that audio and tags the file with Spotify's details. The queue shows which YouTube upload was used.
          </p>
          <p className="flex gap-2">
            <Music className="mt-0.5 size-4 shrink-0 text-muted" />
            <span>Quality is what YouTube offers, usually Opus or AAC around 130–160 kbps. Converting to FLAC, WAV or 320 kbps MP3 does not add detail.</span>
          </p>
          <p>Without Spotify API credentials, albums and playlists are read from Spotify's public player, which lists at most 100 songs. Add your own credentials in Settings → Downloads to read longer playlists in full.</p>
        </div>
      </Card>

      <Card className="mt-4 p-5">
        <SectionTitle>
          <span className="flex items-center gap-2">
            <Scale className="size-4" /> Licenses
          </span>
        </SectionTitle>
        <p className="mb-3 text-[13px] text-fg-2">DarkLoad is released under the MIT License. It builds on the following projects:</p>
        <table className="w-full text-left text-[12px]">
          <tbody className="divide-y divide-line">
            {NOTICES.map(([name, license, role]) => (
              <tr key={name}>
                <td className="py-1.5 pr-3 text-fg">{name}</td>
                <td className="py-1.5 pr-3 text-fg-2">{license}</td>
                <td className="py-1.5 text-muted">{role}</td>
              </tr>
            ))}
          </tbody>
        </table>
        <p className="mt-3 text-xs text-muted">Complete notices are listed in THIRD_PARTY_NOTICES.md in the source repository and installation folder.</p>
      </Card>

      <Card className="mt-4 p-5">
        <SectionTitle>Changelog</SectionTitle>
        {info ? <Changelog markdown={info.changelog} /> : null}
      </Card>
    </div>
  );
}
