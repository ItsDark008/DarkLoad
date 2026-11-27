// Interface translations. English strings are the message keys: `t("Paste")` returns the text for
// the active language and falls back to English when a translation is missing. `{name}`
// placeholders are filled from `params`.
import { create } from "zustand";
import type { LanguagePreference } from "@/types/api";
import { backend as itBackend } from "./it-backend";
import { it } from "./it";

export type Locale = "en" | "it";
export type Params = Record<string, string | number>;

const catalogs: Record<Locale, Readonly<Record<string, string>>> = { en: {}, it };
const backendCatalogs: Record<Locale, Readonly<Record<string, string>>> = { en: {}, it: itBackend };

/** BCP 47 tags used for dates, numbers and language names. */
const INTL_TAGS: Record<Locale, string> = { en: "en-US", it: "it-IT" };

// Until settings load (for example on the startup screen), follow the operating system language.
let current: Locale = resolveLocale("system");
if (typeof document !== "undefined") document.documentElement.lang = current;

export const useLocale = create<{ locale: Locale }>(() => ({ locale: current }));

/** The interface language for a preference; "system" follows the operating system language. */
export function resolveLocale(preference: LanguagePreference | undefined, preferred: readonly string[] = systemLanguages()): Locale {
  if (preference === "en" || preference === "it") return preference;
  for (const tag of preferred) {
    const base = tag.toLowerCase().split("-")[0];
    if (base === "it") return "it";
    if (base === "en") return "en";
  }
  return "en";
}

function systemLanguages(): readonly string[] {
  if (typeof navigator === "undefined") return [];
  return navigator.languages?.length ? navigator.languages : [navigator.language];
}

export function setLocale(locale: Locale) {
  if (locale === current) return;
  current = locale;
  if (typeof document !== "undefined") document.documentElement.lang = locale;
  useLocale.setState({ locale });
}

export function getLocale(): Locale {
  return current;
}

export function intlLocale(): string {
  return INTL_TAGS[current];
}

function fill(template: string, params?: Params): string {
  if (!params) return template;
  return template.replace(/\{(\w+)\}/g, (match, key: string) => (key in params ? String(params[key]) : match));
}

/** Translates an English interface string into the active language. */
export function t(text: string, params?: Params): string {
  return fill(catalogs[current][text] ?? text, params);
}

/**
 * Like {@link t}, for words whose translation depends on where they appear, such as a status
 * ("Completed" on one download) versus a column of counts ("Completed" downloads). The catalog
 * key is `context|text`; without one, the plain text is used.
 */
export function tc(context: string, text: string, params?: Params): string {
  const catalog = catalogs[current];
  return fill(catalog[`${context}|${text}`] ?? catalog[text] ?? text, params);
}

/** Backend messages with `{name}` placeholders, as anchored patterns, most specific first. */
interface Pattern {
  regex: RegExp;
  names: string[];
  template: string;
}

const patternCache = new Map<Locale, Pattern[]>();

function escapeRegex(text: string): string {
  return text.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

function backendPatterns(locale: Locale): Pattern[] {
  let patterns = patternCache.get(locale);
  if (!patterns) {
    patterns = Object.entries(backendCatalogs[locale])
      .filter(([key]) => /\{\w+\}/.test(key))
      .map(([key, template]) => {
        const names: string[] = [];
        const source = key
          .split(/(\{\w+\})/)
          .map((part) => {
            const name = /^\{(\w+)\}$/.exec(part)?.[1];
            if (!name) return escapeRegex(part);
            names.push(name);
            return "(.+?)";
          })
          .join("");
        return { regex: new RegExp(`^${source}$`, "s"), names, template, literal: key.replace(/\{\w+\}/g, "").length };
      })
      .sort((a, b) => b.literal - a.literal)
      .map(({ regex, names, template }) => ({ regex, names, template }));
    patternCache.set(locale, patterns);
  }
  return patterns;
}

function translateBackend(text: string, depth: number): string {
  const known = backendCatalogs[current][text] ?? catalogs[current][text];
  if (known !== undefined) return known;
  if (depth > 2 || current === "en") return text;
  for (const pattern of backendPatterns(current)) {
    const match = pattern.regex.exec(text);
    if (!match) continue;
    const params: Params = {};
    pattern.names.forEach((name, i) => {
      params[name] = translateBackend(match[i + 1] ?? "", depth + 1);
    });
    return fill(pattern.template, params);
  }
  return text;
}

/**
 * Translates text that comes from the backend (errors, notes, labels). Known messages are
 * translated, including those that carry details such as a file name; anything else is shown
 * as the backend wrote it.
 */
export function tr(text: string): string;
export function tr(text: string | null | undefined): string | null;
export function tr(text: string | null | undefined): string | null {
  if (text == null) return null;
  return translateBackend(text, 0);
}

/** Subscribes a component to language changes and returns the translate function. */
export function useT(): typeof t {
  useLocale((s) => s.locale);
  return t;
}
