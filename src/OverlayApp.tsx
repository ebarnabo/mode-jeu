import { useEffect, useMemo, useState, type ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { AppWindow, EyeOff } from "lucide-react";
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

  const notchOn = !!cfg?.notch_visible;
  const pollMs = notchOn ? 4000 : Math.max(2000, cfg?.interval_ms || 2000);

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
  }, [cfg, pollMs]);

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

    return (
      <div
        className={cn(
          "pointer-events-none flex h-full w-full select-none bg-transparent px-2",
          bottom ? "items-end pt-2" : "items-start pb-2",
          align
        )}
      >
        <div
          role="button"
          tabIndex={0}
          className={cn(
            "overlay-notch pointer-events-auto flex cursor-pointer items-center gap-2.5 border-white/10 px-3 py-1.5 shadow-[0_12px_28px_rgba(0,0,0,.4)]",
            bottom
              ? "rounded-t-[18px] border border-b-0"
              : "rounded-b-[18px] border border-t-0",
            origin === "left" && "overlay-notch--left",
            origin === "right" && "overlay-notch--right",
            origin === "bottom" && "overlay-notch--bottom",
            origin === "bottom-left" && "overlay-notch--bottom-left",
            origin === "bottom-right" && "overlay-notch--bottom-right"
          )}
          style={{ background: `rgba(8, 9, 12, ${Math.max(0.82, opacity)})` }}
          onClick={() => run("app", "open_main")}
          onKeyDown={(e) => {
            if (e.key === "Enter" || e.key === " ") {
              e.preventDefault();
              void run("app", "open_main");
            }
          }}
          title="Ouvrir Mode Jeu"
        >
          <span className="h-1.5 w-1.5 shrink-0 rounded-full bg-brass shadow-[0_0_6px_rgba(214,166,74,.85)]" />
          <span className="font-display text-[10px] font-semibold uppercase tracking-[0.18em] text-brass/90">
            MJ
          </span>
          <span className="h-4 w-px bg-white/10" />
          <span className="font-mono text-[11px] font-semibold tabular-nums text-paper">
            {m ? pair(m.ram_used_gb, m.ram_total_gb, 1) : "—"}
            <span className="ml-1 text-[9px] font-medium text-muted">RAM</span>
          </span>
          <span className="font-mono text-[11px] font-semibold tabular-nums text-paper">
            {m ? pair(m.cpu_pct, 100, 0) : "—"}
            <span className="ml-1 text-[9px] font-medium text-muted">CPU</span>
          </span>
          <span
            onClick={(e) => e.stopPropagation()}
            className="ml-0.5 flex items-center"
          >
            <NotchBtn
              label="Masquer"
              disabled={busy !== null}
              onClick={() => run("hide", "hide_notch")}
            >
              <EyeOff className="h-3 w-3" />
            </NotchBtn>
            <NotchBtn
              label="Ouvrir"
              disabled={busy !== null}
              onClick={() => run("app", "open_main")}
            >
              <AppWindow className="h-3 w-3" />
            </NotchBtn>
          </span>
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
      className="flex h-7 w-7 items-center justify-center rounded-full text-paper/70 transition-[transform,background-color,color] duration-[150ms] ease-out hover:bg-white/8 hover:text-paper active:scale-[0.97] disabled:opacity-40"
    >
      {children}
    </button>
  );
}
