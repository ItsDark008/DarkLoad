import { useApp } from "@/stores/app";
import type { Settings } from "@/types/api";
import { useAppearance, useBackendEvents, useClipboardWatcher, useCloseGuard, useGlobalShortcuts, useUpdateNotifications } from "./useAppEffects";

/** Wires every application-level side effect in one place. */
export function useAppEffectsBundle(settings: Settings | null, askToClose: () => Promise<boolean>) {
  const info = useApp((s) => s.info);
  useBackendEvents();
  useAppearance(settings);
  useGlobalShortcuts();
  useClipboardWatcher(settings?.clipboardMonitoring ?? false);
  useCloseGuard(settings?.confirmExitWithActiveDownloads ?? true, askToClose);
  useUpdateNotifications(info?.updaterConfigured);
}
