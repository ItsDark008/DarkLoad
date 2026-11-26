import { create } from "zustand";

/** Incremented whenever download history changes, so views can refresh their queries. */
export const useHistorySignal = create<{ version: number; bump: () => void }>((set) => ({
  version: 0,
  bump: () => set((s) => ({ version: s.version + 1 })),
}));
