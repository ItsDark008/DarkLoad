import { useCallback, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { toast } from "sonner";
import { api, events } from "@/services/api";
import { PLATFORM_NAMES } from "@/lib/urls";
import type { AccountStatus, Platform } from "@/types/api";

/** Live status of the optional platform accounts. */
export function useAccounts() {
  const [accounts, setAccounts] = useState<AccountStatus[] | null>(null);
  const refresh = useCallback(() => {
    api
      .accountsStatus()
      .then(setAccounts)
      .catch(() => setAccounts([]));
  }, []);

  useEffect(() => {
    refresh();
    const unlisten = listen(events.accountsChanged, refresh);
    return () => void unlisten.then((fn) => fn());
  }, [refresh]);

  const connect = useCallback((platform: Platform) => {
    api
      .accountsConnect(platform)
      .then(() => toast(`Sign in to ${PLATFORM_NAMES[platform]}`, { description: "The window closes by itself once you are signed in." }))
      .catch((e: { message?: string }) => toast.error("Could not open the sign-in window", { description: e.message }));
  }, []);

  const disconnect = useCallback(
    (platform: Platform) => {
      api
        .accountsDisconnect(platform)
        .then(() => {
          toast.success(`${PLATFORM_NAMES[platform]} disconnected`);
          refresh();
        })
        .catch((e: { message?: string }) => toast.error("Could not disconnect", { description: e.message }));
    },
    [refresh],
  );

  return { accounts, connect, disconnect, refresh };
}

export function supportsAccount(platform: Platform | null): platform is "tiktok" | "instagram" {
  return platform === "tiktok" || platform === "instagram";
}
