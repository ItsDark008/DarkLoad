import { useEffect, useRef, useState } from "react";
import { Toaster } from "sonner";
import { Loader2 } from "lucide-react";
import { ErrorBoundary } from "@/components/common";
import { LogoMark } from "@/components/brand";
import { TooltipProvider } from "@/components/ui/controls";
import { ConfirmDialog, Dialog, DialogContent } from "@/components/ui/overlays";
import { Button } from "@/components/ui/button";
import { useAppEffectsBundle } from "@/hooks/useAppBundle";
import { AppLayout } from "@/layouts/AppLayout";
import { AboutPage, ShortcutList } from "@/pages/About";
import { BatchPage } from "@/pages/Batch";
import { DownloaderPage } from "@/pages/Downloader";
import { HistoryPage } from "@/pages/History";
import { OverviewPage } from "@/pages/Overview";
import { QueuePage } from "@/pages/Queue";
import { SettingsPage } from "@/pages/Settings";
import { useApp } from "@/stores/app";
import type { AppError } from "@/types/api";

function Splash({ error, onRetry }: { error: AppError | null; onRetry: () => void }) {
  return (
    <div className="flex h-full flex-col items-center justify-center gap-4 bg-bg">
      <LogoMark className="size-12 text-fg" />
      {error ? (
        <div className="max-w-sm text-center">
          <p className="text-[14px] font-medium">DarkLoad could not start</p>
          <p className="mt-1 text-[13px] text-fg-2" data-selectable>
            {error.message}
          </p>
          <Button className="mt-4" variant="primary" onClick={onRetry}>
            Try again
          </Button>
        </div>
      ) : (
        <Loader2 className="size-5 animate-spin text-fg-2" aria-label="Loading" />
      )}
    </div>
  );
}

export function App() {
  const page = useApp((s) => s.page);
  const settings = useApp((s) => s.settings);
  const bootstrap = useApp((s) => s.bootstrap);
  const shortcutsOpen = useApp((s) => s.shortcutsOpen);
  const setShortcutsOpen = useApp((s) => s.setShortcutsOpen);
  const [error, setError] = useState<AppError | null>(null);
  const [closeAsk, setCloseAsk] = useState(false);
  const resolver = useRef<((v: boolean) => void) | null>(null);

  useEffect(() => {
    bootstrap().catch((e: AppError) => setError(e));
  }, [bootstrap]);
  const retry = () => {
    setError(null);
    bootstrap().catch((e: AppError) => setError(e));
  };

  useAppEffectsBundle(
    settings,
    () =>
      new Promise<boolean>((resolve) => {
        resolver.current = resolve;
        setCloseAsk(true);
      }),
  );

  if (!settings) return <Splash error={error} onRetry={retry} />;

  return (
    <TooltipProvider delayDuration={400}>
      <AppLayout>
        <ErrorBoundary key={page}>
          <div className="h-full overflow-y-auto">
            {page === "overview" ? <OverviewPage /> : null}
            {page === "downloader" ? <DownloaderPage /> : null}
            {page === "batch" ? <BatchPage /> : null}
            {page === "queue" ? <QueuePage /> : null}
            {page === "history" ? <HistoryPage /> : null}
            {page === "settings" ? <SettingsPage /> : null}
            {page === "about" ? <AboutPage /> : null}
          </div>
        </ErrorBoundary>
      </AppLayout>
      <Dialog open={shortcutsOpen} onOpenChange={setShortcutsOpen}>
        <DialogContent title="Keyboard shortcuts">
          <ShortcutList />
        </DialogContent>
      </Dialog>
      <ConfirmDialog
        open={closeAsk}
        onOpenChange={(open) => {
          if (!open) {
            setCloseAsk(false);
            resolver.current?.(false);
            resolver.current = null;
          }
        }}
        title="Quit while downloading?"
        description="Active downloads will stop. If “Resume the queue on startup” is on, they continue the next time you open DarkLoad."
        confirmLabel="Quit"
        destructive
        onConfirm={() => {
          resolver.current?.(true);
          resolver.current = null;
          setCloseAsk(false);
        }}
      />
      <Toaster
        theme="dark"
        position="bottom-right"
        toastOptions={{
          classNames: {
            toast: "!bg-elevated !border !border-line-strong !text-fg !rounded-lg !shadow-2xl !shadow-black !font-sans",
            description: "!text-fg-2",
            actionButton: "!bg-accent !text-accent-fg",
          },
        }}
      />
    </TooltipProvider>
  );
}
