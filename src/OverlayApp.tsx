import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { cn } from "@/lib/utils";

type OverlayConfig = {
  enabled: boolean;
  always: boolean;
  show_cpu: boolean;
  show_gpu: boolean;
  show_temps: boolean;
  show_fps: boolean;
  show_ram: boolean;
  opacity: number;
  interval_ms: number;
  hidden: boolean;
};

type Metrics = {
  cpu_pct: number;
  ram_pct: number;
  gpu_pct: number | null;
  cpu_temp_c: number | null;
  gpu_temp_c: number | null;
  fps: number | null;
};

function fmt(n: number | null | undefined, digits = 0) {
  if (n == null || Number.isNaN(n)) return "—";
  return n.toFixed(digits);
}

export default function OverlayApp() {
  const [cfg, setCfg] = useState<OverlayConfig | null>(null);
  const [metrics, setMetrics] = useState<Metrics | null>(null);

  useEffect(() => {
    invoke<OverlayConfig>("get_overlay_config").then(setCfg).catch(() => {});
    const un = listen<OverlayConfig>("overlay-config", (e) => setCfg(e.payload));
    return () => {
      un.then((f) => f()).catch(() => {});
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
    const id = window.setInterval(tick, Math.max(1000, cfg.interval_ms || 2000));
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
      const cpu = metrics.cpu_temp_c == null ? "—" : `${fmt(metrics.cpu_temp_c, 0)}°`;
      const gpu = metrics.gpu_temp_c == null ? "—" : `${fmt(metrics.gpu_temp_c, 0)}°`;
      out.push({ label: "TEMP", value: `${cpu} / ${gpu}` });
    }
    if (cfg.show_ram) {
      out.push({ label: "RAM", value: `${fmt(metrics.ram_pct, 0)}%` });
    }
    return out;
  }, [cfg, metrics]);

  if (!cfg) return null;

  const opacity = Math.min(100, Math.max(35, cfg.opacity)) / 100;

  return (
    <div
      className="h-full w-full select-none px-1 py-1"
      style={{ background: "transparent" }}
    >
      <div
        className="h-full rounded-2xl border border-white/10 px-3 py-2 shadow-[0_12px_40px_rgba(0,0,0,.45)]"
        style={{ background: `rgba(14, 15, 18, ${opacity})` }}
      >
        <div className="mb-1 text-[10px] font-semibold uppercase tracking-[0.16em] text-brass/80">
          Mode Jeu
        </div>
        <div className="flex flex-col gap-1">
          {rows.map((r) => (
            <div key={r.label} className="flex items-baseline justify-between gap-3">
              <span className="text-[11px] font-medium text-muted">{r.label}</span>
              <span
                className={cn(
                  "font-mono text-[15px] font-semibold tabular-nums tracking-tight",
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
