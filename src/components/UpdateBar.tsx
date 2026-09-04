import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { getVersion } from "@tauri-apps/api/app";
import { Loader2, RefreshCw } from "lucide-react";
import { Button } from "@/components/ui/button";

type Phase = "idle" | "checking" | "upToDate" | "downloading" | "ready" | "installing" | "error";

type UpdateState = {
  phase: Phase;
  version: string;
  currentVersion: string;
  progress: number;
  message: string;
  checkNow: (manual: boolean) => Promise<void>;
  install: () => Promise<void>;
};

const UpdateCtx = createContext<UpdateState | null>(null);

export function UpdateProvider({
  gameMode,
  onRestore,
  children,
}: {
  gameMode: boolean;
  onRestore: () => Promise<void>;
  children: ReactNode;
}) {
  const [phase, setPhase] = useState<Phase>("idle");
  const [version, setVersion] = useState("");
  const [currentVersion, setCurrentVersion] = useState("");
  const [progress, setProgress] = useState(0);
  const [message, setMessage] = useState("");
  const updateRef = useRef<Update | null>(null);
  const started = useRef(false);
  const gameModeRef = useRef(gameMode);
  const onRestoreRef = useRef(onRestore);
  gameModeRef.current = gameMode;
  onRestoreRef.current = onRestore;

  const downloadUpdate = useCallback(async (update: Update) => {
    updateRef.current = update;
    setVersion(update.version);
    setPhase("downloading");
    setProgress(0);
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
  }, []);

  const checkNow = useCallback(
    async (manual: boolean) => {
      setPhase("checking");
      setMessage("");
      try {
        const update = await check();
        if (!update) {
          setPhase("upToDate");
          if (manual) setMessage("Tu as déjà la dernière version.");
          return;
        }
        await downloadUpdate(update);
      } catch {
        setPhase("error");
        setMessage(
          manual
            ? "Impossible de vérifier (réseau ou release manquante)."
            : "Vérification automatique indisponible"
        );
      }
    },
    [downloadUpdate]
  );

  useEffect(() => {
    void getVersion()
      .then(setCurrentVersion)
      .catch(() => setCurrentVersion("?"));
  }, []);

  useEffect(() => {
    if (started.current) return;
    started.current = true;
    const timer = window.setTimeout(() => {
      void checkNow(false);
    }, 2500);
    return () => window.clearTimeout(timer);
  }, [checkNow]);

  const install = useCallback(async () => {
    const update = updateRef.current;
    if (!update) return;
    setPhase("installing");
    try {
      if (gameModeRef.current) await onRestoreRef.current();
      await update.install();
      await relaunch();
    } catch {
      setPhase("ready");
      setMessage("Installation échouée — réessaie.");
    }
  }, []);

  const value = useMemo(
    () => ({
      phase,
      version,
      currentVersion,
      progress,
      message,
      checkNow,
      install,
    }),
    [phase, version, currentVersion, progress, message, checkNow, install]
  );

  return <UpdateCtx.Provider value={value}>{children}</UpdateCtx.Provider>;
}

function useUpdateState() {
  const ctx = useContext(UpdateCtx);
  if (!ctx) throw new Error("UpdateProvider manquant");
  return ctx;
}

export function UpdateBar({ gameMode }: { gameMode: boolean }) {
  const state = useUpdateState();

  if (
    state.phase === "idle" ||
    state.phase === "upToDate" ||
    state.phase === "checking" ||
    (state.phase === "error" && !state.version)
  ) {
    return null;
  }

  return (
    <div className="animate-rise mx-8 mb-2 overflow-hidden rounded-2xl border border-line bg-raised">
      {state.phase === "downloading" && (
        <div
          className="h-0.5 bg-brass transition-[width] duration-200"
          style={{ width: `${state.progress}%` }}
        />
      )}
      <div className="flex items-center justify-between gap-4 px-4 py-3">
        <p className="min-w-0 text-[13px] text-paper">
          {state.phase === "downloading" && `Téléchargement de la v${state.version}…`}
          {state.phase === "ready" &&
            (gameMode
              ? `v${state.version} prête — sera installée après ta session`
              : `v${state.version} prête à installer`)}
          {state.phase === "installing" && "Installation en cours…"}
          {state.phase === "error" && (state.message || "Mise à jour indisponible")}
        </p>
        {state.phase === "ready" && (
          <Button size="sm" onClick={() => void state.install()} disabled={gameMode}>
            <RefreshCw className="h-3.5 w-3.5" />
            Redémarrer
          </Button>
        )}
        {state.phase === "installing" && (
          <Loader2 className="h-4 w-4 shrink-0 animate-spin text-brass" />
        )}
      </div>
    </div>
  );
}

export function UpdatesPanel({ gameMode }: { gameMode: boolean }) {
  const state = useUpdateState();

  return (
    <div className="overflow-hidden rounded-2xl border border-line bg-surface/60">
      <div className="flex items-start justify-between gap-4 px-4 py-4">
        <div className="min-w-0">
          <p className="text-[13px] font-semibold text-paper">Mises à jour</p>
          <p className="mt-1 text-[12px] leading-relaxed text-muted">
            Version installée{" "}
            <span className="text-paper/90">{state.currentVersion || "…"}</span>
            {state.phase === "upToDate" && " — à jour"}
            {state.phase === "ready" && ` — v${state.version} prête`}
            {state.phase === "downloading" && ` — téléchargement ${state.progress}%`}
            {state.phase === "checking" && " — vérification…"}
            {state.phase === "error" && state.message ? ` — ${state.message}` : null}
            {state.phase === "idle" && " — en attente"}
          </p>
          <p className="mt-2 text-[11px] text-muted/80">
            Téléchargement silencieux, installation au redémarrage.
          </p>
        </div>
        <div className="flex shrink-0 flex-col gap-2">
          {(state.phase === "idle" ||
            state.phase === "upToDate" ||
            state.phase === "error") && (
            <Button
              size="sm"
              variant="outline"
              onClick={() => void state.checkNow(true)}
            >
              <RefreshCw className="h-3.5 w-3.5" />
              Vérifier
            </Button>
          )}
          {state.phase === "ready" && (
            <Button size="sm" onClick={() => void state.install()} disabled={gameMode}>
              <RefreshCw className="h-3.5 w-3.5" />
              Installer
            </Button>
          )}
          {(state.phase === "downloading" ||
            state.phase === "installing" ||
            state.phase === "checking") && (
            <Loader2 className="h-4 w-4 self-center animate-spin text-brass" />
          )}
        </div>
      </div>
      {state.phase === "downloading" && (
        <div className="h-0.5 bg-line">
          <div
            className="h-full bg-brass transition-[width] duration-200"
            style={{ width: `${state.progress}%` }}
          />
        </div>
      )}
    </div>
  );
}
