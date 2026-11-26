import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";

export function cn(...inputs: ClassValue[]): string {
  return twMerge(clsx(inputs));
}

export function randomId(): string {
  return typeof crypto !== "undefined" && "randomUUID" in crypto
    ? crypto.randomUUID()
    : `${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 10)}`;
}

function clampByte(n: number): number {
  return Math.max(0, Math.min(255, Math.round(n)));
}

/** Mixes a hex color toward white by `amount` (0–1). */
export function lighten(hex: string, amount: number): string {
  const match = /^#([0-9a-f]{6})$/i.exec(hex);
  if (!match?.[1]) return hex;
  const value = parseInt(match[1], 16);
  const channels = [(value >> 16) & 255, (value >> 8) & 255, value & 255].map((c) => clampByte(c + (255 - c) * amount));
  return `#${channels.map((c) => c.toString(16).padStart(2, "0")).join("")}`.toUpperCase();
}

/** Relative luminance as defined by WCAG 2.x. */
export function luminance(hex: string): number {
  const match = /^#([0-9a-f]{6})$/i.exec(hex);
  if (!match?.[1]) return 0;
  const value = parseInt(match[1], 16);
  const [r, g, b] = [(value >> 16) & 255, (value >> 8) & 255, value & 255].map((c) => {
    const s = c / 255;
    return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
  }) as [number, number, number];
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

export function contrastRatio(a: string, b: string): number {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x) as [number, number];
  return (hi + 0.05) / (lo + 0.05);
}

/**
 * Picks black or white label text for a filled accent surface. White is preferred while it keeps
 * at least 4:1 contrast (labels on filled buttons are medium weight); otherwise black is used.
 */
export function readableTextOn(hex: string): "#000000" | "#FFFFFF" {
  return contrastRatio(hex, "#FFFFFF") >= 4 ? "#FFFFFF" : "#000000";
}

/** Lightening steps that turn the base color into the layered OLED surfaces. */
const SURFACE_STEPS = { card: 0.031, elevated: 0.063, hover: 0.078, border: 0.11, borderStrong: 0.165 } as const;

export const MAX_OLED_BACKGROUND_LUMINANCE = 0.02;

/**
 * Derives every OLED surface token from one base color. With pure black this reproduces the
 * default palette exactly (#080808 cards, #101010 elevated surfaces, #1C1C1C borders).
 */
export function oledPalette(base: string): Record<string, string> {
  return {
    "--dl-bg": base.toUpperCase(),
    "--dl-sidebar": base.toUpperCase(),
    "--dl-card": lighten(base, SURFACE_STEPS.card),
    "--dl-elevated": lighten(base, SURFACE_STEPS.elevated),
    "--dl-hover": lighten(base, SURFACE_STEPS.hover),
    "--dl-border": lighten(base, SURFACE_STEPS.border),
    "--dl-border-strong": lighten(base, SURFACE_STEPS.borderStrong),
  };
}

/** Whether a color is dark enough to be an OLED background (mirrors the backend rule). */
export function isValidOledBackground(hex: string): boolean {
  return /^#[0-9a-f]{6}$/i.test(hex) && luminance(hex) <= MAX_OLED_BACKGROUND_LUMINANCE;
}

export const isMac = typeof navigator !== "undefined" && /Mac/i.test(navigator.platform);
export const modKey = isMac ? "⌘" : "Ctrl";
