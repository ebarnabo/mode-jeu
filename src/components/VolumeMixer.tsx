import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Volume2, VolumeX } from "lucide-react";
import { cn } from "@/lib/utils";

export type AppVolume = {
  id: string;
  name: string;
  volume: number;
  muted: boolean;
};

export function VolumeMixer({ compact }: { compact?: boolean }) {
  const [apps, setApps] = useState<AppVolume[]>([]);
  const [loaded, setLoaded] = useState(false);
  const dragging = useRef<string | null>(null);
  const lastSend = useRef(0);

  const refresh = useCallback(async () => {
    if (dragging.current) return;
    try {
      setApps(await invoke<AppVolume[]>("list_app_volumes"));
    } catch {
      /* silencieux */
    } finally {
      setLoaded(true);
    }
  }, []);

  useEffect(() => {
    void refresh();
    const id = window.setInterval(() => void refresh(), compact ? 2800 : 2200);
    return () => window.clearInterval(id);
  }, [refresh, compact]);

  const slide = (id: string, volume: number) => {
    dragging.current = id;
    setApps((prev) =>
      prev.map((a) => (a.id === id ? { ...a, volume, muted: false } : a))
    );
    const now = Date.now();
    if (now - lastSend.current < 90) return;
    lastSend.current = now;
    invoke<AppVolume[]>("set_app_volume", { id, volume }).catch(() => {});
  };

  const commit = (id: string, volume: number) => {
    dragging.current = null;
    invoke<AppVolume[]>("set_app_volume", { id, volume })
      .then(setApps)
      .catch(() => {});
  };

  const toggleMute = (app: AppVolume) => {
    invoke<AppVolume[]>("set_app_mute", { id: app.id, muted: !app.muted })
      .then(setApps)
      .catch(() => {});
  };

  const body =
    !loaded ? (
      <p className={cn(compact ? "px-1 py-1 text-[11px] text-muted" : "px-4 py-8 text-center text-[13px] text-muted")}>
        Lecture des sessions audio…
      </p>
    ) : apps.length === 0 ? (
      <p className={cn(compact ? "px-1 py-1 text-[11px] text-muted" : "px-4 py-8 text-center text-[13px] text-muted")}>
        Aucune app ne joue de son.
      </p>
    ) : (
      <ul className={cn(compact && "max-h-[144px] overflow-y-auto")}>
        {apps.map((app, i) => (
          <li
            key={app.id}
            className={cn(
              "flex items-center gap-2",
              compact ? "px-0.5 py-1" : "px-4 py-3",
              !compact && i > 0 && "border-t border-line/80"
            )}
          >
            <span
              className={cn(
                "min-w-0 truncate font-medium text-paper",
                compact ? "w-[9.5ch] text-[11px]" : "w-[14ch] text-[14px]",
                app.muted && "text-muted"
              )}
              title={app.name}
            >
              {app.name}
            </span>
            <input
              type="range"
              min={0}
              max={100}
              value={app.volume}
              className={cn("mixer-range min-w-0 flex-1", app.muted && "opacity-40")}
              aria-label={`Volume ${app.name}`}
              onPointerDown={(e) => e.stopPropagation()}
              onChange={(e) => slide(app.id, Number(e.target.value))}
              onPointerUp={(e) => {
                e.stopPropagation();
                commit(app.id, Number((e.target as HTMLInputElement).value));
              }}
            />
            <span
              className={cn(
                "w-8 shrink-0 text-right font-mono tabular-nums text-muted",
                compact ? "text-[10px]" : "text-[12px]"
              )}
            >
              {app.muted ? "—" : app.volume}
            </span>
            <button
              type="button"
              aria-label={app.muted ? `Activer le son de ${app.name}` : `Couper ${app.name}`}
              className={cn(
                "flex shrink-0 items-center justify-center rounded-full text-muted hover:text-paper",
                compact ? "h-6 w-6" : "h-8 w-8 hover:bg-white/5"
              )}
              onClick={(e) => {
                e.stopPropagation();
                toggleMute(app);
              }}
            >
              {app.muted ? <VolumeX className="h-3.5 w-3.5" /> : <Volume2 className="h-3.5 w-3.5" />}
            </button>
          </li>
        ))}
      </ul>
    );

  if (compact) return body;

  return (
    <section>
      <h2 className="font-display text-lg font-semibold text-paper">Son</h2>
      <p className="mt-1 text-[13px] text-muted">
        Volume de chaque app qui a une session audio. Rien n’est estimé.
      </p>
      <div className="mt-4 overflow-hidden rounded-2xl border border-line bg-surface/60">{body}</div>
    </section>
  );
}
