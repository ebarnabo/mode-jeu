import { useEffect, useMemo, useState, type ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { AppWindow, EyeOff, RotateCcw } from "lucide-react";
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
  opacity: number;
  interval_ms: number;
  hidden: boolean;
  session_active: boolean;
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

  useEffect(() => {
    invoke<OverlayConfig>("get_overlay_config").then(setCfg).catch(() => {});
    const unCfg = listen<OverlayConfig>("overlay-config", (e) => setCfg(e.payload));
    const unSess = listen<{ active: boolean }>("session-changed", (e) => {
      setCfg((prev) => (prev ? { ...prev, session_active: e.payload.active } : prev));
    });
    return () => {
      unCfg.then((f) => f()).catch(() => {});
      unSess.then((f) => f()).catch(() => {});
    };
  }, []);

  useEffect(() => {
    if (!cfg) return;
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
    const id = window.setInterval(tick, Math.max(2000, cfg.interval_ms || 2000));
    const onVis = () => {
      if (!document.hidden) void tick();
    };
    document.addEventListener("visibilitychange", onVis);
    return () => {
      alive = false;
      window.clearInterval(id);
      document.removeEventListener("visibilitychange", onVis);
    };
  }, [cfg?.interval_ms]);

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

  if (!cfg) return null;

  const opacity = Math.min(100, Math.max(35, cfg.opacity)) / 100;
  const notchOn = cfg.notch !== false && !!cfg.session_active;

  if (notchOn) {
    const m = metrics;
    return (
      <div className="pointer-events-none flex h-full w-full select-none items-start justify-center bg-transparent px-4 pb-3">
        <div
          className="overlay-notch pointer-events-auto flex items-center gap-3 rounded-b-[28px] border border-t-0 border-white/12 px-3.5 py-2.5 shadow-[0_18px_40px_rgba(0,0,0,.45)]"
          style={{ background: `rgba(14, 15, 18, ${opacity})` }}
        >
          <div className="flex items-center gap-2 pr-1">
            <span className="h-1.5 w-1.5 rounded-full bg-brass shadow-[0_0_8px_rgba(214,166,74,.9)]" />
            <span className="font-display text-[11px] font-semibold uppercase tracking-[0.16em] text-brass/90">
              Jeu
            </span>
          </div>

          <div className="h-6 w-px bg-white/10" />

          <div className="flex items-center gap-3">
            {cfg.show_ram && (
              <NotchStat
                label="RAM"
                value={m ? pair(m.ram_used_gb, m.ram_total_gb, 1) : "—"}
                hint="Go"
              />
            )}
            {cfg.show_cpu && (
              <NotchStat
                label="CPU"
                value={m ? pair(m.cpu_pct, 100, 0) : "—"}
                hint={
                  cfg.show_temps && m?.cpu_temp_c != null
                    ? `${fmt(m.cpu_temp_c, 0)}°`
                    : undefined
                }
              />
            )}
            {cfg.show_gpu && (
              <NotchStat
                label="GPU"
                value={m?.gpu_pct != null ? pair(m.gpu_pct, 100, 0) : "—"}
                hint={
                  cfg.show_temps && m?.gpu_temp_c != null
                    ? `${fmt(m.gpu_temp_c, 0)}°`
                    : m?.gpu_used_gb != null && m?.gpu_total_gb != null
                      ? `${fmt(m.gpu_used_gb, 1)}/${fmt(m.gpu_total_gb, 1)}`
                      : undefined
                }
              />
            )}
            {cfg.show_fps && (
              <NotchStat label="FPS" value={m ? fmt(m.fps, 0) : "—"} accent />
            )}
          </div>

          <div className="h-6 w-px bg-white/10" />

          <div className="flex items-center gap-1">
            <NotchBtn
              label="Restaurer"
              disabled={busy !== null}
              onClick={() => run("restore", "restore")}
            >
              <RotateCcw className="h-3.5 w-3.5" />
            </NotchBtn>
            <NotchBtn
              label="Masquer"
              disabled={busy !== null}
              onClick={() => run("hide", "toggle_overlay_visibility")}
            >
              <EyeOff className="h-3.5 w-3.5" />
            </NotchBtn>
            <NotchBtn
              label="App"
              disabled={busy !== null}
              onClick={() => run("app", "open_main")}
            >
              <AppWindow className="h-3.5 w-3.5" />
            </NotchBtn>
          </div>
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
        <div className="mb-2 text-[11px] font-semibold uppercase tracking-[0.18em] text-brass/85">
          Mode Jeu
        </div>
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

function NotchStat({
  label,
  value,
  hint,
  accent,
}: {
  label: string;
  value: string;
  hint?: string;
  accent?: boolean;
}) {
  return (
    <div className="min-w-[52px]">
      <div className="flex items-baseline justify-between gap-1.5">
        <span className="text-[9px] font-semibold uppercase tracking-[0.14em] text-muted">
          {label}
        </span>
        {hint ? <span className="text-[9px] tabular-nums text-muted/80">{hint}</span> : null}
      </div>
      <div
        className={cn(
          "mt-0.5 font-mono text-[13px] font-semibold tabular-nums leading-none tracking-tight",
          accent ? "text-brass" : "text-paper"
        )}
      >
        {value}
      </div>
    </div>
  );
}

function NotchBtn({
  label,
  children,
  onClick,
  disabled,
}: {
  label: string;
  children: ReactNode;
  onClick: () => void;
  disabled?: boolean;
}) {
  return (
    <button
      type="button"
      title={label}
      aria-label={label}
      disabled={disabled}
      onClick={(e) => {
        e.stopPropagation();
        onClick();
      }}
      className="flex h-8 w-8 items-center justify-center rounded-full text-paper/80 transition-[transform,background-color,color] duration-[150ms] ease-out hover:bg-white/8 hover:text-paper active:scale-[0.97] disabled:opacity-40"
    >
      {children}
    </button>
  );
}
