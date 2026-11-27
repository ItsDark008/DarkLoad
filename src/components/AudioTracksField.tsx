import { useState } from "react";
import { ChevronDown, Search } from "lucide-react";
import { Checkbox } from "@/components/ui/controls";
import { Input } from "@/components/ui/input";
import { intlLocale, useT } from "@/i18n";
import type { AudioTrackSelection, MediaFormat } from "@/types/api";

const languageCode = /^[a-z]{2,3}(?:-[a-z0-9]{1,8})*$/i;
const commonLanguages = [
  "af", "am", "ar", "az", "be", "bg", "bn", "bs", "ca", "ceb", "cs", "cy", "da", "de", "el", "en", "en-GB", "en-US",
  "es", "es-419", "et", "eu", "fa", "fi", "fil", "fr", "fr-CA", "gl", "gu", "he", "hi", "hr", "hu", "hy", "id", "is", "it",
  "ja", "jv", "ka", "kk", "km", "kn", "ko", "lo", "lt", "lv", "mk", "ml", "mn", "mr", "ms", "my", "ne", "nl", "no", "pa",
  "pl", "pt", "pt-BR", "pt-PT", "ro", "ru", "si", "sk", "sl", "sq", "sr", "sv", "sw", "ta", "te", "th", "tr", "uk", "ur", "uz",
  "vi", "yue", "zh", "zh-Hans", "zh-Hant",
].map((code) => code.toLowerCase());

const languageNames = new Map<string, Intl.DisplayNames>();

function displayNames(): Intl.DisplayNames {
  const locale = intlLocale();
  let names = languageNames.get(locale);
  if (!names) {
    names = new Intl.DisplayNames([locale], { type: "language" });
    languageNames.set(locale, names);
  }
  return names;
}

function languageLabel(code: string, t: (text: string) => string): string {
  const descriptive = code.endsWith("-desc");
  const base = descriptive ? code.slice(0, -5) : code;
  try {
    const name = displayNames().of(base) ?? base;
    return descriptive ? `${name} (${t("descriptive")})` : name;
  } catch {
    return code;
  }
}

function searchText(value: string): string {
  return value.normalize("NFD").replace(/\p{M}/gu, "").toLowerCase().trim();
}

export function AudioTracksField({ formats, selection, video, onChange }: {
  formats?: MediaFormat[];
  selection: AudioTrackSelection;
  video: boolean;
  onChange: (selection: AudioTrackSelection) => void;
}) {
  const t = useT();
  const [query, setQuery] = useState("");
  const labeled = formats?.flatMap((format) => format.acodec && format.language && languageCode.test(format.language)
    ? [{ language: format.language.toLowerCase(), original: format.languagePreference === 10 || /\(original\)|\[original\]/i.test(format.note ?? "") }]
    : []) ?? [];
  const offered = [...new Set(labeled.map((track) => track.language))];
  const originalLanguage = labeled.find((track) => track.original)?.language;
  const selected = selection.languages.map((language) => language.toLowerCase());
  const codes = [...new Set([...(formats ? offered : commonLanguages), ...selected])]
    .map((code) => ({ code, label: languageLabel(code, t) }))
    .sort((a, b) => a.label.localeCompare(b.label));
  const filter = searchText(query);
  const visibleLanguages = codes.filter(({ code, label }) => code.includes(filter) || searchText(label).includes(filter));
  const labels = [...(selection.original ? [t("Original")] : []), ...selected.map((code) => languageLabel(code, t))];

  const toggleLanguage = (language: string, checked: boolean) => {
    if (!checked) onChange({ ...selection, languages: selected.filter((value) => value !== language) });
    else if (!video) onChange({ original: false, languages: [language] });
    else if (selected.length < 20) onChange({ ...selection, languages: [...selected, language] });
  };
  return (
    <details className="group rounded-lg border border-line">
      <summary className="flex cursor-pointer list-none items-center gap-2 px-3 py-2.5 text-[13px] font-medium text-fg focus-visible:outline-accent [&::-webkit-details-marker]:hidden">
        <span className="shrink-0">{t("Audio tracks")}</span>
        <span className="ml-auto truncate font-normal text-fg-2" title={labels.join(", ")}>{labels.join(", ") || t("Automatic")}</span>
        <ChevronDown className="size-4 shrink-0 text-fg-2 group-open:rotate-180" />
      </summary>
      <fieldset className="space-y-2 border-t border-line px-3 py-2.5">
        <legend className="sr-only">{t("Audio tracks")}</legend>
        <p className="text-xs text-fg-2">{video ? t("Choose the tracks to include.") : t("Choose one track.")} {formats ? t("Clear your selection to use Automatic.") : t("Availability is checked for each download.")}</p>
        <label className="flex min-w-0 items-center gap-2 text-[13px] text-fg">
          <Checkbox checked={selection.original} onCheckedChange={(checked) => onChange({ original: checked === true, languages: video ? selected : [] })} />
          <span className="truncate">{originalLanguage ? `${t("Original")} · ${languageLabel(originalLanguage, t)}` : t("Original")}</span>
        </label>
        {codes.length > 0 ? (
          <div className="relative">
            <Search aria-hidden className="pointer-events-none absolute top-2 left-2.5 size-4 text-fg-2" />
            <Input type="search" aria-label={t("Search languages")} className="h-8 pl-8" placeholder={t("Search languages")} value={query} onChange={(event) => setQuery(event.target.value)} />
          </div>
        ) : null}
        <div className="grid max-h-52 grid-cols-2 gap-x-3 gap-y-2 overflow-y-auto">
          {visibleLanguages.map(({ code, label }) => (
            <label key={code} className="flex min-w-0 items-center gap-2 text-[13px] text-fg" title={label}>
              <Checkbox checked={selected.includes(code)} disabled={video && !selected.includes(code) && selected.length >= 20} onCheckedChange={(checked) => toggleLanguage(code, checked === true)} />
              <span className="truncate">{label}</span>
            </label>
          ))}
        </div>
        {codes.length > 0 && visibleLanguages.length === 0 ? <p role="status" className="text-xs text-fg-2">{t("No matching languages.")}</p> : null}
        {formats && !offered.length ? <p className="text-xs text-fg-2">{t("The source did not report audio languages.")}</p> : null}
      </fieldset>
    </details>
  );
}
