import { readText } from "@tauri-apps/plugin-clipboard-manager";
import { detectPlatform, extractUrls } from "./urls";

/** Reads the clipboard and returns the first supported link, if any. */
export async function readClipboardUrl(): Promise<string | null> {
  try {
    const text = (await readText()) ?? "";
    const trimmed = text.trim();
    return extractUrls(text)[0] ?? (trimmed.length < 2048 && detectPlatform(trimmed) ? trimmed : null);
  } catch {
    return null;
  }
}
