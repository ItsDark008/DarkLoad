import { forwardRef, type ReactNode } from "react";
import { ClipboardPaste, X } from "lucide-react";
import { PlatformIcon } from "@/components/brand";
import { Button } from "@/components/ui/button";
import { Tooltip } from "@/components/ui/controls";
import { detectPlatform } from "@/lib/urls";
import { readClipboardUrl } from "@/lib/clipboard";
import { cn, modKey } from "@/lib/utils";
import { useT } from "@/i18n";

interface UrlFieldProps {
  value: string;
  onChange: (value: string) => void;
  onSubmit: (value: string) => void;
  action?: ReactNode;
  size?: "md" | "lg";
  placeholder?: string;
  disabled?: boolean;
  autoFocus?: boolean;
}

export const UrlField = forwardRef<HTMLInputElement, UrlFieldProps>(function UrlField(
  { value, onChange, onSubmit, action, size = "lg", placeholder, disabled, autoFocus },
  ref,
) {
  const t = useT();
  const platform = detectPlatform(value);
  const paste = async () => {
    const url = await readClipboardUrl();
    if (url) {
      onChange(url);
      onSubmit(url);
    }
  };
  return (
    <form
      className={cn(
        "flex items-center gap-2 rounded-xl border border-line bg-card pr-1.5 pl-3.5 transition-colors focus-within:border-accent focus-within:ring-2 focus-within:ring-accent/20 hover:border-line-strong",
        size === "lg" ? "h-14" : "h-11",
      )}
      onSubmit={(e) => {
        e.preventDefault();
        if (value.trim()) onSubmit(value.trim());
      }}
    >
      <PlatformIcon platform={platform} className={cn("size-5 transition-colors", platform ? "text-fg" : "text-muted")} />
      <input
        ref={ref}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        placeholder={placeholder ?? t("Paste a YouTube, TikTok, Instagram or Spotify link")}
        aria-label={t("Media link")}
        disabled={disabled}
        autoFocus={autoFocus}
        spellCheck={false}
        autoComplete="off"
        maxLength={2048}
        className={cn("h-full min-w-0 flex-1 bg-transparent text-fg outline-none placeholder:text-muted", size === "lg" ? "text-[14px]" : "text-[13px]")}
      />
      {value ? (
        <Button type="button" size="icon-sm" variant="ghost" aria-label={t("Clear link")} onClick={() => onChange("")}>
          <X />
        </Button>
      ) : null}
      <Tooltip content={t("Paste from clipboard")} shortcut={`${modKey}+Shift+V`}>
        <Button type="button" size={size === "lg" ? "md" : "sm"} variant="ghost" onClick={() => void paste()} disabled={disabled}>
          <ClipboardPaste /> {t("Paste")}
        </Button>
      </Tooltip>
      {action}
    </form>
  );
});
