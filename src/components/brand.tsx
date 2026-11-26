import { cn } from "@/lib/utils";
import type { Platform } from "@/types/api";

/** The DarkLoad mark: an open hexagon with a downward arrow passing through it. */
export function LogoMark({ className, title = "DarkLoad" }: { className?: string; title?: string }) {
  return (
    <svg viewBox="200 140 624 720" className={cn("size-6", className)} role="img" aria-label={title}>
      <path
        d="M584.7 254 L771.8 362 L771.8 662 L512 812 L252.2 662 L252.2 362 L439.3 254"
        fill="none"
        stroke="var(--dl-accent)"
        strokeWidth="64"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
      <path d="M512 168 V 606" fill="none" stroke="currentColor" strokeWidth="72" strokeLinecap="round" />
      <path d="M398 516 L512 630 L626 516" fill="none" stroke="currentColor" strokeWidth="72" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  );
}

/** Simple monochrome platform glyphs (original artwork, not official logos). */
export function PlatformIcon({ platform, className }: { platform: Platform | null; className?: string }) {
  const common = { className: cn("size-4 shrink-0", className), viewBox: "0 0 24 24", fill: "none", stroke: "currentColor", strokeWidth: 1.8, "aria-hidden": true } as const;
  switch (platform) {
    case "youtube":
      return (
        <svg {...common}>
          <rect x="2.5" y="5" width="19" height="14" rx="4" />
          <path d="M10 9.2v5.6l4.8-2.8z" fill="currentColor" stroke="none" />
        </svg>
      );
    case "tiktok":
      return (
        <svg {...common}>
          <path d="M14 3.5v11.2a3.7 3.7 0 1 1-3.7-3.7" strokeLinecap="round" />
          <path d="M14 3.5c.5 2.6 2.3 4.2 5 4.4" strokeLinecap="round" />
        </svg>
      );
    case "instagram":
      return (
        <svg {...common}>
          <rect x="3.5" y="3.5" width="17" height="17" rx="5" />
          <circle cx="12" cy="12" r="3.8" />
          <circle cx="17.1" cy="6.9" r="0.9" fill="currentColor" stroke="none" />
        </svg>
      );
    default:
      return (
        <svg {...common}>
          <path d="M10 14a4 4 0 0 0 5.7 0l3-3a4 4 0 0 0-5.7-5.7l-1 1" strokeLinecap="round" />
          <path d="M14 10a4 4 0 0 0-5.7 0l-3 3a4 4 0 0 0 5.7 5.7l1-1" strokeLinecap="round" />
        </svg>
      );
  }
}
