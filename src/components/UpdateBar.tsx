import { useEffect, useRef, useState } from "react";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { Loader2, RefreshCw } from "lucide-react";
import { Button } from "@/components/ui/button";

type Phase = "idle" | "downloading" | "ready" | "installing";

export function UpdateBar({
  gameMode,
  onRestore,
}: {
  gameMode: boolean;
  onRestore: () => Promise<void>;
}) {
  const [phase, setPhase] = useState<Phase>("idle");
  const [version, setVersion] = useState("");
  const [progress, setProgress] = useState(0);
  const updateRef = useRef<Update | null>(null);

  useEffect(() => {
    const timer = window.setTimeout(() => {
      void (async () => {
        try {
          const update = await check();
          if (!update) return;
          updateRef.current = update;
          setVersion(update.version);
          setPhase("downloading");
          let downloaded = 0;
          let total = 0;
          await update.download((event) => {
            if (event.event === "Started") {
              total = event.data.contentLength ?? 0;
            } else if (event.event === "Progress") {
              downloaded += event.data.chunkLength;
              if (total > 0) {
                setProgress(Math.min(100, Math.round((downloaded / total) * 100)));
              }
            } else if (event.event === "Finished") {
              setProgress(100);
            }
          });
          setPhase("ready");
        } catch {
          setPhase("idle");
        }
      })();
    }, 2500);
    return () => window.clearTimeout(timer);
  }, []);

  const install = async () => {
    const update = updateRef.current;
    if (!update) return;
    setPhase("installing");
    try {
      if (gameMode) await onRestore();
      await update.install({ restartAfterInstall: true });
      await relaunch();
    } catch {
      setPhase("ready");
    }
  };

  if (phase === "idle") return null;

  return (
    <div className="animate-rise mx-8 mb-2 overflow-hidden rounded-2xl border border-line bg-raised">
      {phase === "downloading" && (
        <div
          className="h-0.5 bg-brass transition-[width] duration-200"
          style={{ width: `${progress}%` }}
        />
      )}
      <div className="flex items-center justify-between gap-4 px-4 py-3">
        <p className="min-w-0 text-[13px] text-paper">
          {phase === "downloading" && `Téléchargement de la v${version}…`}
          {phase === "ready" &&
            (gameMode
              ? `v${version} prête — sera installée après ta session`
              : `v${version} prête à installer`)}
          {phase === "installing" && "Installation en cours…"}
        </p>
        {phase === "ready" && (
          <Button size="sm" onClick={() => void install()} disabled={gameMode}>
            <RefreshCw className="h-3.5 w-3.5" />
            Redémarrer
          </Button>
        )}
        {phase === "installing" && <Loader2 className="h-4 w-4 shrink-0 animate-spin text-brass" />}
      </div>
    </div>
  );
}
