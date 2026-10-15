import { useEffect, useMemo, useState, type MouseEvent, type PointerEvent, type ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { EyeOff, File, Folder, Pause, Play, X } from "lucide-react";
import { NotchMascot } from "@/components/NotchMascot";
import { cn } from "@/lib/utils";

type OverlayConfig = {
  enabled: boolean;
  always: boolean;
  show_cpu: boolean;
  show_gpu: boolean;
  show_temps: boolean;
  show_fps: boolean;
  show_ram: boolean;
  notch: boolean;
  notch_position: string;
  opacity: number;
  interval_ms: number;
  hidden: boolean;
  session_active: boolean;
  notch_visible: boolean;
};

type Metrics = {
  cpu_pct: number;
  ram_pct: number;
  ram_used_gb: number;
  ram_total_gb: number;
  gpu_pct: number | null;
  gpu_used_gb: number | null;
  gpu_total_gb: number | null;
  cpu_temp_c: number | null;
  gpu_temp_c: number | null;
  fps: number | null;
};

type MediaNow = {
  playing: boolean;
  title: string | null;
  artist: string | null;
};

type ShelfItem = {
  id: number;
  path: string;
  name: string;
  is_dir: boolean;
};

function fmt(n: number | null | undefined, digits = 0) {
  if (n == null || Number.isNaN(n)) return "—";
  return n.toFixed(digits);
}

function pair(used: number | null | undefined, total: number | null | undefined, digits = 0) {
  if (used == null || total == null || Number.isNaN(used) || Number.isNaN(total)) return "—";
  return `${fmt(used, digits)}/${fmt(total, digits)}`;
}

export default function OverlayApp() {
  const [cfg, setCfg] = useState<OverlayConfig | null>(null);
  const [metrics, setMetrics] = useState<Metrics | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [media, setMedia] = useState<MediaNow>({ playing: false, title: null, artist: null });
  const [shelf, setShelf] = useState<ShelfItem[]>([]);
  const [dropHover, setDropHover] = useState(false);

  useEffect(() => {
    invoke<OverlayConfig>("get_overlay_config").then(setCfg).catch(() => {});
    invoke<ShelfItem[]>("shelf_list").then(setShelf).catch(() => {});
    const unCfg = listen<OverlayConfig>("overlay-config", (e) => setCfg(e.payload));
    const unSess = listen<{ active: boolean }>("session-changed", (e) => {
      setCfg((prev) => (prev ? { ...prev, session_active: e.payload.active } : prev));
    });
    const unShelf = listen<ShelfItem[]>("shelf-changed", (e) => setShelf(e.payload));
    const unDrag = listen<boolean>("notch-drag", (e) => setDropHover(!!e.payload));
    return () => {
      unCfg.then((f) => f()).catch(() => {});
      unSess.then((f) => f()).catch(() => {});
      unShelf.then((f) => f()).catch(() => {});
      unDrag.then((f) => f()).catch(() => {});
    };
  }, []);

  const notchOn = !!cfg?.notch_visible;
  const overlayLive =
    notchOn ||
    (!!cfg?.enabled && !cfg?.hidden && (!!cfg?.session_active || !!cfg?.always));
  const pollMs = notchOn ? 5000 : Math.max(2500, cfg?.interval_ms || 3000);

  useEffect(() => {
    if (!cfg || !overlayLive) return;
    let alive = true;
    const tick = async () => {
      if (document.hidden) return;
      try {
        const m = await invoke<Metrics>("get_metrics");
        if (alive) setMetrics(m);
      } catch {
        /* silencieux */
      }
    };
    tick();
    const id = window.setInterval(tick, pollMs);
    const onVis = () => {
      if (!document.hidden) void tick();
    };
    document.addEventListener("visibilitychange", onVis);
    return () => {
      alive = false;
      window.clearInterval(id);
      document.removeEventListener("visibilitychange", onVis);
    };
  }, [cfg, pollMs, overlayLive]);

  useEffect(() => {
    if (!notchOn) return;
    let alive = true;
    const tick = async () => {
      if (document.hidden) return;
      try {
        const n = await invoke<MediaNow>("get_media");
        if (alive) setMedia(n);
      } catch {
        /* silencieux */
      }
    };
    tick();
    const id = window.setInterval(tick, 4000);
    return () => {
      alive = false;
      window.clearInterval(id);
    };
  }, [notchOn]);

  const rows = useMemo(() => {
    if (!cfg || !metrics) return [];
    const out: { label: string; value: string; accent?: boolean }[] = [];
    if (cfg.show_fps) {
      out.push({ label: "FPS", value: fmt(metrics.fps, 0), accent: true });
    }
    if (cfg.show_cpu) {
      out.push({ label: "CPU", value: `${fmt(metrics.cpu_pct, 0)}%` });
    }
    if (cfg.show_gpu) {
      out.push({
        label: "GPU",
        value: metrics.gpu_pct == null ? "—" : `${fmt(metrics.gpu_pct, 0)}%`,
      });
    }
    if (cfg.show_temps) {
      out.push({
        label: "CPU °C",
        value: metrics.cpu_temp_c == null ? "—" : `${fmt(metrics.cpu_temp_c, 0)}°`,
      });
      out.push({
        label: "GPU °C",
        value: metrics.gpu_temp_c == null ? "—" : `${fmt(metrics.gpu_temp_c, 0)}°`,
      });
    }
    if (cfg.show_ram) {
      out.push({ label: "RAM", value: `${fmt(metrics.ram_pct, 0)}%` });
    }
    return out;
  }, [cfg, metrics]);

  const run = async (id: string, cmd: string) => {
    if (busy) return;
    setBusy(id);
    try {
      await invoke(cmd);
    } catch {
      /* silencieux */
    } finally {
      setBusy(null);
    }
  };

  const toggleMedia = async (e: MouseEvent) => {
    e.stopPropagation();
    try {
      setMedia(await invoke<MediaNow>("media_toggle"));
    } catch {
      /* silencieux */
    }
  };

  const dragShelf = async (e: PointerEvent, id: number) => {
    e.stopPropagation();
    if (e.button !== 0) return;
    try {
      await invoke("shelf_drag", { id });
    } catch {
      /* silencieux */
    }
  };

  const removeShelf = async (e: MouseEvent, id: number) => {
    e.stopPropagation();
    try {
      setShelf(await invoke<ShelfItem[]>("shelf_remove", { id }));
    } catch {
      /* silencieux */
    }
  };

  if (!cfg) return null;
  if (!overlayLive) return null;

  const opacity = Math.min(100, Math.max(35, cfg.opacity)) / 100;

  if (notchOn) {
    const m = metrics;
    const pos = cfg.notch_position || "top-center";
    const bottom = pos.startsWith("bottom");
    const align =
      pos.endsWith("left")
        ? "justify-start"
        : pos.endsWith("right")
          ? "justify-end"
          : "justify-center";
    const origin = bottom
      ? pos.endsWith("left")
        ? "bottom-left"
        : pos.endsWith("right")
          ? "bottom-right"
          : "bottom"
      : pos.endsWith("left")
        ? "left"
        : pos.endsWith("right")
          ? "right"
          : "center";
    const roomy = dropHover || shelf.length > 0;
    const mediaLabel = media.title
      ? `${media.playing ? "Pause" : "Lecture"} · ${media.title}${media.artist ? ` — ${media.artist}` : ""}`
      : media.playing
        ? "Pause"
        : "Lecture";

    return (
      <div
        className={cn(
          "pointer-events-none flex h-full w-full select-none bg-transparent px-2",
          bottom ? "items-end pt-2" : "items-start pb-2",
          align
        )}
      >
        <div
          className={cn(
            "overlay-notch pointer-events-auto flex max-w-full cursor-default flex-col gap-1.5 border-white/10 px-3 py-1.5 shadow-[0_12px_28px_rgba(0,0,0,.4)]",
            bottom
              ? "rounded-t-[18px] border border-b-0"
              : "rounded-b-[18px] border border-t-0",
            bottom && roomy && "flex-col-reverse",
            origin === "left" && "overlay-notch--left",
            origin === "right" && "overlay-notch--right",
            origin === "bottom" && "overlay-notch--bottom",
            origin === "bottom-left" && "overlay-notch--bottom-left",
            origin === "bottom-right" && "overlay-notch--bottom-right",
            roomy && "overlay-notch--roomy"
          )}
          style={{ background: `rgba(8, 9, 12, ${Math.max(0.82, opacity)})` }}
        >
          <div className="flex items-center gap-2.5">
            <NotchMascot
              cpu={m?.cpu_pct ?? null}
              ram={m?.ram_pct ?? null}
              playing={media.playing}
              holding={shelf.length > 0}
              catching={dropHover}
              track={media.title}
              onClick={() => run("app", "open_main")}
            />
            <span className="h-4 w-px bg-white/10" />
            <span className="font-mono text-[11px] font-semibold tabular-nums text-paper">
              {m ? pair(m.ram_used_gb, m.ram_total_gb, 1) : "—"}
              <span className="ml-1 text-[9px] font-medium text-muted">RAM</span>
            </span>
            <span className="font-mono text-[11px] font-semibold tabular-nums text-paper">
              {m ? pair(m.cpu_pct, 100, 0) : "—"}
              <span className="ml-1 text-[9px] font-medium text-muted">CPU</span>
            </span>
            <span className="h-4 w-px bg-white/10" />
            <NotchBtn label={mediaLabel} onClick={toggleMedia} hot={media.playing}>
              {media.playing ? <Pause className="h-3 w-3" /> : <Play className="h-3 w-3" />}
            </NotchBtn>
            <span className="ml-auto flex items-center">
              <NotchBtn
                label="Masquer"
                disabled={busy !== null}
                onClick={() => run("hide", "hide_notch")}
              >
                <EyeOff className="h-3 w-3" />
              </NotchBtn>
            </span>
          </div>

          {roomy && (
            <div
              className={cn(
                "flex max-w-[520px] items-center gap-1.5 overflow-x-auto pb-0.5",
                dropHover && shelf.length === 0 && "min-h-[28px]"
              )}
            >
              {shelf.map((it) => (
                <div
                  key={it.id}
                  title={`${it.path}\nGlisse vers un dossier pour y déposer.`}
                  onPointerDown={(e) => void dragShelf(e, it.id)}
                  className="flex max-w-[140px] shrink-0 cursor-grab items-center gap-1 rounded-full border border-white/10 bg-white/5 py-0.5 pl-1.5 pr-0.5 active:cursor-grabbing"
                >
                  {it.is_dir ? (
                    <Folder className="h-3 w-3 shrink-0 text-brass" />
                  ) : (
                    <File className="h-3 w-3 shrink-0 text-paper/70" />
                  )}
                  <span className="truncate text-[10px] font-medium text-paper">{it.name}</span>
                  <button
                    type="button"
                    aria-label="Retirer"
                    className="flex h-4 w-4 items-center justify-center rounded-full text-muted hover:text-paper"
                    onPointerDown={(e) => e.stopPropagation()}
                    onClick={(e) => void removeShelf(e, it.id)}
                  >
                    <X className="h-2.5 w-2.5" />
                  </button>
                </div>
              ))}
              {dropHover && (
                <span className="shrink-0 px-1 text-[10px] font-medium text-brass/90">
                  Dépose ici
                </span>
              )}
            </div>
          )}
        </div>
      </div>
    );
  }

  return (
    <div
      className="h-full w-full select-none px-1.5 py-1.5"
      style={{ background: "transparent" }}
    >
      <div
        className="flex h-full flex-col justify-center rounded-2xl border border-white/12 px-4 py-3 shadow-[0_16px_48px_rgba(0,0,0,.5)]"
        style={{ background: `rgba(14, 15, 18, ${opacity})` }}
      >
        <div className="flex flex-col gap-1.5">
          {rows.map((r) => (
            <div key={r.label} className="flex items-baseline justify-between gap-4">
              <span className="text-[12px] font-medium text-muted">{r.label}</span>
              <span
                className={cn(
                  "font-mono text-[20px] font-semibold tabular-nums tracking-tight leading-none",
                  r.accent ? "text-brass" : "text-paper"
                )}
              >
                {r.value}
              </span>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}

function NotchBtn({
  label,
  children,
  onClick,
  disabled,
  hot,
}: {
  label: string;
  children: ReactNode;
  onClick: (e: MouseEvent<HTMLButtonElement>) => void;
  disabled?: boolean;
  hot?: boolean;
}) {
  return (
    <button
      type="button"
      title={label}
      aria-label={label}
      disabled={disabled}
      onClick={(e) => {
        e.stopPropagation();
        onClick(e);
      }}
      className={cn(
        "flex h-7 w-7 items-center justify-center rounded-full transition-[transform,background-color,color] duration-[150ms] ease-out hover:bg-white/8 active:scale-[0.97] disabled:opacity-40",
        hot ? "text-brass hover:text-brass" : "text-paper/70 hover:text-paper"
      )}
    >
      {children}
    </button>
  );
}
