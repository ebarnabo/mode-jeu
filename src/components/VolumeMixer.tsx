import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ChevronDown, Speaker, Volume2, VolumeX } from "lucide-react";
import { cn } from "@/lib/utils";

export type AppVolume = {
  id: string;
  name: string;
  volume: number;
  muted: boolean;
  kind?: string;
};

export type AudioOutput = {
  id: string;
  name: string;
  default: boolean;
};

type Variant = "page" | "compact" | "flyout";

function iconFor(kind?: string) {
  if (kind === "device") return Speaker;
  return Volume2;
}

function fillStyle(volume: number, muted: boolean) {
  const pct = muted ? 0 : volume;
  return {
    background: `linear-gradient(to right, #e2b15a ${pct}%, #2a2a2e ${pct}%)`,
  };
}

function OutputPicker({
  outputs,
  activeId,
  mode,
  onChange,
}: {
  outputs: AudioOutput[];
  activeId: string;
  mode: Variant;
  onChange: (id: string) => void;
}) {
  if (outputs.length === 0) return null;
  const active = outputs.find((o) => o.id === activeId) ?? outputs.find((o) => o.default) ?? outputs[0];

  return (
    <label
      className={cn(
        "relative flex min-w-0 items-center gap-2",
        mode === "compact" && "mb-1 rounded-lg border border-white/10 bg-white/[0.04] px-2 py-1",
        mode === "flyout" && "mb-2 rounded-xl border border-line bg-raised/80 px-2.5 py-2",
        mode === "page" && "mb-3 rounded-xl border border-line bg-raised/60 px-3 py-2.5"
      )}
      onPointerDown={(e) => e.stopPropagation()}
    >
      <Speaker className={cn("shrink-0 text-brass", mode === "compact" ? "h-3 w-3" : "h-4 w-4")} />
      <span
        className={cn(
          "shrink-0 font-semibold uppercase tracking-wide text-muted",
          mode === "compact" ? "text-[9px]" : "text-[10px]"
        )}
      >
        Sortie
      </span>
      <select
        value={active?.id ?? ""}
        onChange={(e) => onChange(e.target.value)}
        className={cn(
          "min-w-0 flex-1 cursor-pointer appearance-none truncate bg-transparent font-medium text-paper outline-none",
          mode === "compact" ? "text-[11px]" : "text-[13px]"
        )}
        aria-label="Choisir la sortie audio"
      >
        {outputs.map((o) => (
          <option key={o.id} value={o.id} className="bg-surface text-paper">
            {o.name}
            {o.default ? " · actuelle" : ""}
          </option>
        ))}
      </select>
      <ChevronDown className={cn("shrink-0 text-muted", mode === "compact" ? "h-3 w-3" : "h-4 w-4")} />
    </label>
  );
}

export function VolumeMixer({
  compact,
  variant,
}: {
  compact?: boolean;
  variant?: Variant;
}) {
  const mode: Variant = variant ?? (compact ? "compact" : "page");
  const [apps, setApps] = useState<AppVolume[]>([]);
  const [outputs, setOutputs] = useState<AudioOutput[]>([]);
  const [outputId, setOutputId] = useState("");
  const [loaded, setLoaded] = useState(false);
  const dragging = useRef<string | null>(null);
  const lastSend = useRef(0);
  const switching = useRef(false);

  const refresh = useCallback(async () => {
    if (dragging.current || switching.current) return;
    if (document.hidden) return;
    try {
      const [vols, outs] = await Promise.all([
        invoke<AppVolume[]>("list_app_volumes"),
        invoke<AudioOutput[]>("list_audio_outputs"),
      ]);
      setApps(vols);
      setOutputs(outs);
      const active = outs.find((o) => o.default) ?? outs[0];
      if (active) setOutputId(active.id);
    } catch {
      /* silencieux */
    } finally {
      setLoaded(true);
    }
  }, []);

  useEffect(() => {
    void refresh();
    const ms = mode === "compact" ? 4200 : mode === "flyout" ? 2600 : 2400;
    const id = window.setInterval(() => void refresh(), ms);
    const onVis = () => {
      if (!document.hidden) void refresh();
    };
    document.addEventListener("visibilitychange", onVis);
    return () => {
      window.clearInterval(id);
      document.removeEventListener("visibilitychange", onVis);
    };
  }, [refresh, mode]);

  const pickOutput = (id: string) => {
    if (!id || id === outputId) return;
    switching.current = true;
    setOutputId(id);
    invoke<AppVolume[]>("set_default_audio_output", { id })
      .then((vols) => {
        setApps(vols);
        return invoke<AudioOutput[]>("list_audio_outputs");
      })
      .then((outs) => {
        setOutputs(outs);
        const active = outs.find((o) => o.default);
        if (active) setOutputId(active.id);
      })
      .catch(() => {
        invoke<AudioOutput[]>("list_audio_outputs")
          .then((outs) => {
            setOutputs(outs);
            const active = outs.find((o) => o.default);
            if (active) setOutputId(active.id);
          })
          .catch(() => {});
      })
      .finally(() => {
        switching.current = false;
      });
  };

  const slide = (id: string, volume: number) => {
    dragging.current = id;
    setApps((prev) =>
      prev.map((a) => (a.id === id ? { ...a, volume, muted: false } : a))
    );
    const now = Date.now();
    if (now - lastSend.current < 80) return;
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

  const emptyCopy =
    mode === "compact" ? "Rien ne joue." : "Aucune app ne joue de son sur cette sortie.";

  const picker = (
    <OutputPicker outputs={outputs} activeId={outputId} mode={mode} onChange={pickOutput} />
  );

  const body =
    !loaded ? (
      <p
        className={cn(
          "text-muted",
          mode === "compact" && "px-1 py-1 text-[11px]",
          mode === "flyout" && "px-1 py-6 text-center text-[13px]",
          mode === "page" && "px-4 py-8 text-center text-[13px]"
        )}
      >
        Lecture des sessions…
      </p>
    ) : apps.length === 0 ? (
      <p
        className={cn(
          "text-muted",
          mode === "compact" && "px-1 py-1 text-[11px]",
          mode === "flyout" && "px-1 py-6 text-center text-[13px]",
          mode === "page" && "px-4 py-8 text-center text-[13px]"
        )}
      >
        {emptyCopy}
      </p>
    ) : (
      <ul
        className={cn(
          mode === "compact" && "max-h-[248px] overflow-y-auto pr-0.5",
          mode === "flyout" && "max-h-[min(420px,calc(100vh-72px))] overflow-y-auto"
        )}
      >
        {apps.map((app, i) => {
          const Icon = iconFor(app.kind);
          const master = app.kind === "device";
          return (
            <li
              key={app.id}
              className={cn(
                "flex items-center gap-2",
                mode === "compact" && "px-0.5 py-1",
                mode === "page" && "px-4 py-3",
                mode === "flyout" && (master ? "px-1 py-2.5" : "px-1 py-1.5"),
                mode === "page" && i > 0 && "border-t border-line/80",
                mode === "flyout" && master && i === 0 && "mb-1 border-b border-white/8 pb-3"
              )}
            >
              {mode !== "compact" && (
                <Icon
                  className={cn(
                    "shrink-0 text-muted",
                    master ? "h-4 w-4 text-brass" : "h-3.5 w-3.5",
                    app.muted && "opacity-40"
                  )}
                />
              )}
              <span
                className={cn(
                  "min-w-0 truncate font-medium text-paper",
                  mode === "compact" && "w-[8.5ch] text-[11px]",
                  mode === "page" && "w-[14ch] text-[14px]",
                  mode === "flyout" && (master ? "w-[11ch] text-[13px] font-semibold" : "w-[10ch] text-[12px]"),
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
                style={fillStyle(app.volume, app.muted)}
                className={cn(
                  "mixer-range min-w-0 flex-1",
                  app.muted && "opacity-40",
                  master && mode !== "compact" && "h-[6px]"
                )}
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
                  mode === "compact" ? "text-[10px]" : "text-[12px]"
                )}
              >
                {app.muted ? "—" : app.volume}
              </span>
              <button
                type="button"
                aria-label={app.muted ? `Activer le son de ${app.name}` : `Couper ${app.name}`}
                className={cn(
                  "flex shrink-0 items-center justify-center rounded-full text-muted hover:text-paper",
                  mode === "compact" ? "h-6 w-6" : "h-8 w-8 hover:bg-white/5"
                )}
                onClick={(e) => {
                  e.stopPropagation();
                  toggleMute(app);
                }}
              >
                {app.muted ? (
                  <VolumeX className={cn(mode === "compact" ? "h-3 w-3" : "h-3.5 w-3.5")} />
                ) : (
                  <Volume2 className={cn(mode === "compact" ? "h-3 w-3" : "h-3.5 w-3.5")} />
                )}
              </button>
            </li>
          );
        })}
      </ul>
    );

  if (mode === "compact") {
    return (
      <div className="min-w-0">
        {picker}
        {body}
      </div>
    );
  }

  if (mode === "flyout") {
    return (
      <div className="flex h-full flex-col">
        <header className="flex items-center gap-2 px-1 pb-2">
          <Speaker className="h-4 w-4 text-brass" />
          <h1 className="font-display text-[15px] font-semibold tracking-tight text-paper">Son</h1>
        </header>
        {picker}
        {body}
      </div>
    );
  }

  return (
    <section>
      <h2 className="font-display text-lg font-semibold text-paper">Son</h2>
      <p className="mt-1 text-[13px] text-muted">
        Sortie Windows, volume maître et apps sur la sortie active. Le mute garde le niveau.
      </p>
      <div className="mt-4 overflow-hidden rounded-2xl border border-line bg-surface/60">
        <div className="border-b border-line/80 px-4 py-3">{picker}</div>
        {body}
      </div>
    </section>
  );
}
