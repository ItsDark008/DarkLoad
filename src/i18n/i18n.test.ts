import { afterEach, describe, expect, it as test } from "vitest";
import { it as italian } from "./it";
import { backend } from "./it-backend";
import { getLocale, intlLocale, resolveLocale, setLocale, t, tc, tr } from "./index";

// Every source file, to find the strings passed to t() and tc().
const sources = import.meta.glob<string>(["/src/**/*.{ts,tsx}", "!/src/**/*.test.{ts,tsx}", "!/src/i18n/it*.ts"], {
  query: "?raw",
  import: "default",
  eager: true,
});

function usedKeys(): Map<string, string> {
  const keys = new Map<string, string>();
  const literal = String.raw`"((?:[^"\\]|\\.)*)"`;
  const plain = new RegExp(String.raw`\bt\(\s*` + literal, "g");
  const withContext = new RegExp(String.raw`\btc\(\s*` + literal + String.raw`\s*,\s*` + literal, "g");
  for (const [file, source] of Object.entries(sources)) {
    for (const m of source.matchAll(plain)) keys.set(JSON.parse(`"${m[1]}"`) as string, file);
    for (const m of source.matchAll(withContext)) keys.set(`${JSON.parse(`"${m[1]}"`) as string}|${JSON.parse(`"${m[2]}"`) as string}`, file);
  }
  return keys;
}

function placeholders(text: string): string[] {
  return [...text.matchAll(/\{(\w+)\}/g)].map((m) => m[1] ?? "").sort();
}

describe("Italian catalog", () => {
  const keys = usedKeys();

  test("finds the interface strings", () => {
    expect(keys.size).toBeGreaterThan(500);
  });

  test("translates every interface string", () => {
    const missing = [...keys].filter(([key]) => !(key in italian)).map(([key, file]) => `${file}: ${key}`);
    expect(missing).toEqual([]);
  });

  test("has no unused entries", () => {
    expect(Object.keys(italian).filter((key) => !keys.has(key))).toEqual([]);
  });

  test("keeps the placeholders of every message", () => {
    const mismatched = Object.entries({ ...italian, ...backend })
      .filter(([key, value]) => placeholders(key.replace(/^\w+\|/, "")).join() !== placeholders(value).join())
      .map(([key]) => key);
    expect(mismatched).toEqual([]);
  });

  test("does not translate backend messages already covered by the interface catalog", () => {
    expect(Object.keys(backend).filter((key) => key in italian)).toEqual([]);
  });
});

describe("translation", () => {
  afterEach(() => setLocale("en"));

  test("resolves the language from the preference and the system", () => {
    expect(resolveLocale("it", ["en-US"])).toBe("it");
    expect(resolveLocale("en", ["it-IT"])).toBe("en");
    expect(resolveLocale("system", ["it-IT", "en-US"])).toBe("it");
    expect(resolveLocale("system", ["de-DE", "it"])).toBe("it");
    expect(resolveLocale("system", ["fr-FR"])).toBe("en");
    expect(resolveLocale(undefined, [])).toBe("en");
  });

  test("returns English by default and fills placeholders", () => {
    expect(getLocale()).toBe("en");
    expect(t("Paste")).toBe("Paste");
    expect(t("{time} left", { time: "5s" })).toBe("5s left");
  });

  test("switches to Italian", () => {
    setLocale("it");
    expect(document.documentElement.lang).toBe("it");
    expect(intlLocale()).toBe("it-IT");
    expect(t("Paste")).toBe("Incolla");
    expect(t("{time} left", { time: "5s" })).toBe("5s rimanenti");
    expect(t("Not in the catalog")).toBe("Not in the catalog");
    expect(tc("count", "Completed")).toBe("Completati");
    expect(t("Completed")).toBe("Completato");
    expect(tr(null)).toBeNull();
  });

  test("translates backend messages, including ones with details", () => {
    expect(tr("The link is too long.")).toBe("The link is too long.");
    setLocale("it");
    expect(tr("The link is too long.")).toBe("Il link è troppo lungo.");
    expect(tr("Best available")).toBe("Migliore disponibile");
    expect(tr("Could not download yt-dlp.")).toBe("Impossibile scaricare yt-dlp.");
    expect(tr("A network request failed. Check your connection and try again. Retrying (2/3)…")).toBe(
      "Una richiesta di rete non è riuscita. Controlla la connessione e riprova. Nuovo tentativo (2/3)…",
    );
    expect(tr("MP3 (highest quality) from Opus 160 kbps")).toBe("MP3 (massima qualità) da Opus 160 kbps");
    expect(tr("MP3 192 kbps from Opus 160 kbps")).toBe("MP3 192 kbps da Opus 160 kbps");
    expect(tr("An unknown message from the backend")).toBe("An unknown message from the backend");
  });
});
