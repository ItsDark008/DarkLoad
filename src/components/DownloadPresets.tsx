import { useId, useState } from "react";
import { Bookmark, Trash2 } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Input, Label } from "@/components/ui/input";
import { Dialog, DialogContent } from "@/components/ui/overlays";
import { Select } from "@/components/ui/select";
import { optionsForPlatform, optionsFromSettings } from "@/lib/options";
import { useApp, type DownloadPresetPage } from "@/stores/app";
import { useT } from "@/i18n";
import type { DownloadOptions, DownloadPreset, Platform } from "@/types/api";

const ORIGINAL_VALUE = "original";
/** Names of the built-in preset in every language, which saved presets may not reuse. */
const ORIGINAL_NAMES = ["original", "originale"];

function savedPresetValue(name: string): string {
  return `saved:${name}`;
}

function isOriginalName(name: string): boolean {
  return ORIGINAL_NAMES.includes(name.toLowerCase());
}

export function DownloadPresets({ page, options, outputDir, platform, onApply }: {
  page: DownloadPresetPage;
  options: DownloadOptions;
  outputDir: string;
  platform?: Platform;
  onApply: (preset: DownloadPreset) => void;
}) {
  const t = useT();
  const settings = useApp((s) => s.settings);
  const selected = useApp((s) => s.selectedPresets[page]);
  const selectPreset = useApp((s) => s.selectPreset);
  const updateSettings = useApp((s) => s.updateSettings);
  const nameId = useId();
  const [open, setOpen] = useState(false);
  const [name, setName] = useState("");
  const [saving, setSaving] = useState(false);
  const cleanedName = name.trim();
  if (!settings) return null;
  const { downloadPresets: presets, downloadDir } = settings;
  const duplicate = isOriginalName(cleanedName) || presets.some((preset) => preset.name.toLowerCase() === cleanedName.toLowerCase());
  const originalName = t("Original");
  const original: DownloadPreset = { name: originalName, options: optionsFromSettings(settings), outputDir: null };
  const originalChoice = { value: ORIGINAL_VALUE, label: originalName, preset: original };
  const choices = [
    originalChoice,
    ...presets.map((preset) => ({
      value: savedPresetValue(preset.name),
      label: isOriginalName(preset.name) ? t("{name} (saved)", { name: preset.name }) : preset.name,
      preset,
    })),
  ];
  const active = choices.find((choice) => choice.value === selected) ?? originalChoice;
  const activeOptions = optionsForPlatform(active.preset.options, platform);
  const matches = (active.preset.outputDir ?? downloadDir) === outputDir
    && (Object.keys(activeOptions) as (keyof DownloadOptions)[]).every((key) => JSON.stringify(options[key]) === JSON.stringify(activeOptions[key]));

  const save = async () => {
    if (!cleanedName || duplicate || saving) return;
    const preset: DownloadPreset = { name: cleanedName, options, outputDir: outputDir || null };
    setSaving(true);
    const saved = await updateSettings((settings) => ({ downloadPresets: [...settings.downloadPresets, preset] }));
    setSaving(false);
    if (saved) {
      selectPreset(page, savedPresetValue(preset.name));
      setName("");
      setOpen(false);
      toast.success(t("Saved preset “{name}”", { name: preset.name }));
    }
  };

  const remove = async (preset: DownloadPreset) => {
    setSaving(true);
    const saved = await updateSettings((settings) => ({ downloadPresets: settings.downloadPresets.filter((entry) => entry.name !== preset.name) }));
    setSaving(false);
    if (saved) toast.success(t("Removed preset “{name}”", { name: preset.name }));
  };

  return (
    <div className="mb-4 flex items-center gap-2">
      <Select
        aria-label={t("Apply a download preset")}
        value={matches ? active.value : ""}
        placeholder={t("Custom")}
        options={choices}
        onValueChange={(value) => {
          const choice = choices.find((entry) => entry.value === value);
          if (!choice) return;
          selectPreset(page, choice.value);
          onApply(choice.preset);
          toast.success(t("Applied preset “{name}”", { name: choice.label }));
        }}
      />
      <Dialog open={open} onOpenChange={(next) => {
        setOpen(next);
        if (!next) setName("");
      }}>
        <Button onClick={() => setOpen(true)}><Bookmark /> {t("Presets")}</Button>
        <DialogContent title={t("Download presets")} description={t("Save the current options and destination as a preset you can use on any download page.")}>
          <form className="space-y-2" onSubmit={(event) => { event.preventDefault(); void save(); }}>
            <Label htmlFor={nameId}>{t("Preset name")}</Label>
            <div className="flex items-center gap-2">
              <Input id={nameId} value={name} maxLength={64} placeholder={t("e.g. MP4 with subtitles")} onChange={(event) => setName(event.target.value)} />
              <Button type="submit" variant="primary" disabled={saving || !cleanedName || duplicate}>{t("Save preset")}</Button>
            </div>
            {duplicate ? <p className="text-xs text-warning">{t("A preset with this name already exists.")}</p> : null}
          </form>
          {presets.length > 0 ? (
            <ul className="mt-4 divide-y divide-line border-t border-line">
              {presets.map((preset) => (
                <li key={preset.name} className="flex items-center justify-between gap-3 py-2">
                  <span className="truncate text-[13px] text-fg" title={preset.name}>{preset.name}</span>
                  <Button size="icon-sm" variant="ghost" disabled={saving} aria-label={t("Remove preset {name}", { name: preset.name })} onClick={() => void remove(preset)}>
                    <Trash2 />
                  </Button>
                </li>
              ))}
            </ul>
          ) : null}
        </DialogContent>
      </Dialog>
    </div>
  );
}
