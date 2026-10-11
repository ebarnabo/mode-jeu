import { useCallback, useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  Gamepad2,
  Lock,
  Search,
  RotateCcw,
  Loader2,
  Shield,
  Monitor,
  Settings2,
  Zap,
  CheckSquare,
  Square,
} from "lucide-react";
import { UpdateBar, UpdateProvider, UpdatesPanel } from "@/components/UpdateBar";
import { PowerIgnition } from "@/components/PowerIgnition";
import { MonitorPicker, type MonitorInfo } from "@/components/MonitorPicker";
import { Button } from "@/components/ui/button";
import { Switch } from "@/components/ui/switch";
import { Input } from "@/components/ui/input";
import { cn } from "@/lib/utils";

type ProcGroup = {
  key: string;
  name: string;
  memory_mb: number;
  cpu_pct: number;
  instances: number;
  path: string | null;
  protected: boolean;
  kept: boolean;
  foreground: boolean;
};

type OverlayConfig = {
  enabled: boolean;
  always: boolean;
  monitor_name: string | null;
  game_monitor_name: string | null;
  show_cpu: boolean;
  show_gpu: boolean;
  show_temps: boolean;
  show_fps: boolean;
  show_ram: boolean;
  opacity: number;
  position: string;
  interval_ms: number;
  hidden: boolean;
};

type Config = {
  keep: string[];
  high_performance: boolean;
  ultimate_performance: boolean;
  protect_foreground: boolean;
  stop_services: boolean;
  services: string[];
  minimize_on_activate: boolean;
  start_with_windows: boolean;
  disable_game_dvr: boolean;
  enable_game_mode: boolean;
  disable_notifications: boolean;
  visual_effects_perf: boolean;
  disable_transparency: boolean;
  disable_widgets: boolean;
  disable_background_apps: boolean;
  mmcss_gaming: boolean;
  min_close_mb: number;
  reopen_closed_apps: boolean;
  overlay: OverlayConfig;
};

type SystemInfo = {
  cpu: string;
  gpu: string;
  ram_gb: number;
  tips: string[];
};

type Tab = "session" | "apps" | "overlay" | "options";
type AppSort = "ram" | "cpu";
type AppFilter = "all" | "closable" | "kept";

const KEEP_PRESETS: { id: string; label: string; keys: string[] }[] = [
  { id: "discord", label: "Discord", keys: ["discord.exe"] },
  {
    id: "browsers",
    label: "Navigateurs",
    keys: ["chrome.exe", "msedge.exe", "firefox.exe", "brave.exe", "opera.exe", "vivaldi.exe"],
  },
  {
    id: "comms",
    label: "Comms",
    keys: ["discord.exe", "slack.exe", "teams.exe", "telegram.exe", "whatsapp.exe"],
  },
  {
    id: "launchers",
    label: "Launchers",
    keys: [
      "steam.exe",
      "epicgameslauncher.exe",
      "battle.net.exe",
      "galaxyclient.exe",
      "riotclientservices.exe",
    ],
  },
];

const TABS: { id: Tab; label: string; icon: typeof Zap }[] = [
  { id: "session", label: "Session", icon: Zap },
  { id: "apps", label: "Apps", icon: Shield },
  { id: "overlay", label: "Overlay", icon: Monitor },
  { id: "options", label: "Options", icon: Settings2 },
];

type Session = {
  active: boolean;
  closed: string[];
  closed_names: string[];
  freed_mb: number;
};

export default function App() {
  const [config, setConfig] = useState<Config | null>(null);
  const [session, setSession] = useState<Session | null>(null);
  const [procs, setProcs] = useState<ProcGroup[]>([]);
  const [monitors, setMonitors] = useState<MonitorInfo[]>([]);
  const [sysInfo, setSysInfo] = useState<SystemInfo | null>(null);
  const [monitorMode, setMonitorMode] = useState<"overlay" | "game">("overlay");
  const [tab, setTab] = useState<Tab>("session");
  const [query, setQuery] = useState("");
  const [busy, setBusy] = useState(false);
  const [appSort, setAppSort] = useState<AppSort>("ram");
  const [appFilter, setAppFilter] = useState<AppFilter>("all");
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [ignition, setIgnition] = useState<{
    freedMb: number;
    appsClosed: number;
  } | null>(null);

  const refresh = useCallback(async () => {
    const [c, s, p, m, info] = await Promise.all([
      invoke<Config>("get_config"),
      invoke<Session>("get_session"),
      invoke<ProcGroup[]>("list_processes"),
      invoke<MonitorInfo[]>("list_monitors").catch(() => [] as MonitorInfo[]),
      invoke<SystemInfo>("get_system_info").catch(() => null),
    ]);
    setConfig(c);
    setSession(s);
    setProcs(p);
    setMonitors(m);
    if (info) setSysInfo(info);
  }, []);

  // Liste processus : une fois au boot (chiffres Session), puis plus souvent sur Apps.
  useEffect(() => {
    refresh();
  }, [refresh]);

  useEffect(() => {
    if (busy) return;
    const tick = () => {
      invoke<ProcGroup[]>("list_processes").then(setProcs).catch(() => {});
    };
    if (tab === "apps") {
      tick();
      const id = setInterval(tick, 10000);
      return () => clearInterval(id);
    }
    // Hors Apps : refresh léger occasionnel pour les totaux Session.
    const id = setInterval(tick, 30000);
    return () => clearInterval(id);
  }, [tab, busy]);

  const patch = async (next: Partial<Config>) => {
    if (!config) return;
    const merged = { ...config, ...next };
    setConfig(merged);
    await invoke("save_config", { config: merged });
  };

  const patchOverlay = (next: Partial<OverlayConfig>) => {
    if (!config) return;
    patch({ overlay: { ...config.overlay, ...next } });
  };

  const persistKeep = async (keep: string[]) => {
    if (!config) return;
    const merged = { ...config, keep };
    setConfig(merged);
    setProcs((prev) => prev.map((p) => ({ ...p, kept: keep.includes(p.key) })));
    await invoke("save_config", { config: merged });
  };

  const toggleKeep = (key: string, keep: boolean) => {
    if (!config) return;
    const next = keep
      ? Array.from(new Set([...config.keep, key]))
      : config.keep.filter((k) => k !== key);
    void persistKeep(next);
  };

  const applyPreset = (keys: string[], enable: boolean) => {
    if (!config) return;
    let next = [...config.keep];
    if (enable) {
      for (const k of keys) {
        if (!next.includes(k)) next.push(k);
      }
    } else {
      next = next.filter((k) => !keys.includes(k));
    }
    void persistKeep(next);
  };

  const applyKeepSelection = (keep: boolean) => {
    if (!config || selected.size === 0) return;
    let next = [...config.keep];
    for (const key of selected) {
      if (keep && !next.includes(key)) next.push(key);
      if (!keep) next = next.filter((k) => k !== key);
    }
    setSelected(new Set());
    void persistKeep(next);
  };

  const applyAmdProfile = () => {
    if (!config) return;
    void patch({
      high_performance: true,
      ultimate_performance: true,
      enable_game_mode: true,
      disable_game_dvr: true,
      disable_widgets: true,
      disable_notifications: true,
      visual_effects_perf: true,
      disable_transparency: true,
      disable_background_apps: true,
      mmcss_gaming: true,
      stop_services: true,
      min_close_mb: 40,
    });
  };

  const clearIgnition = useCallback(() => setIgnition(null), []);

  const run = async (cmd: "activate" | "restore") => {
    setBusy(true);
    try {
      const next = await invoke<Session>(cmd);
      setSession(next);
      if (cmd === "activate") {
        setIgnition({
          freedMb: next.freed_mb,
          appsClosed: next.closed_names.length,
        });
        // Si on minimise, pas besoin de rescanner tous les process tout de suite.
        if (!config?.minimize_on_activate) {
          setProcs(await invoke<ProcGroup[]>("list_processes"));
        }
      } else {
        setProcs(await invoke<ProcGroup[]>("list_processes"));
      }
    } finally {
      setBusy(false);
    }
  };

  const active = session?.active ?? false;

  const { closable, reclaimable } = useMemo(() => {
    const minMb = config?.min_close_mb ?? 40;
    const list = procs.filter(
      (p) =>
        !p.protected &&
        !p.kept &&
        !(config?.protect_foreground && p.foreground) &&
        p.memory_mb >= minMb
    );
    return { closable: list.length, reclaimable: list.reduce((n, p) => n + p.memory_mb, 0) };
  }, [procs, config]);

  const visible = useMemo(() => {
    const q = query.trim().toLowerCase();
    const minMb = config?.min_close_mb ?? 40;
    let list = procs.filter((p) => !p.protected && (!q || p.name.toLowerCase().includes(q)));
    if (appFilter === "kept") list = list.filter((p) => p.kept);
    if (appFilter === "closable") {
      list = list.filter(
        (p) => !p.kept && !(config?.protect_foreground && p.foreground) && p.memory_mb >= minMb
      );
    }
    return [...list].sort((a, b) => {
      if (a.kept !== b.kept) return a.kept ? -1 : 1;
      if (appSort === "cpu") {
        return b.cpu_pct - a.cpu_pct || b.memory_mb - a.memory_mb;
      }
      return b.memory_mb - a.memory_mb || b.cpu_pct - a.cpu_pct;
    });
  }, [procs, query, appSort, appFilter, config]);

  const toggleSelect = (key: string) => {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });
  };

  if (!config) {
    return (
      <div className="flex h-full items-center justify-center bg-ink">
        <Loader2 className="h-5 w-5 animate-spin text-muted" />
      </div>
    );
  }

  return (
    <UpdateProvider
      gameMode={active}
      onRestore={async () => {
        await run("restore");
      }}
    >
    <div className="app-shell relative flex h-full flex-col overflow-hidden">
      <PowerIgnition
        open={!!ignition}
        freedMb={ignition?.freedMb ?? 0}
        appsClosed={ignition?.appsClosed ?? 0}
        onDone={clearIgnition}
      />
      <header
        data-tauri-drag-region
        className="relative z-10 flex items-start justify-between gap-4 px-7 pb-4 pt-7"
      >
        <div>
          <p className="text-[11px] font-semibold uppercase tracking-[0.22em] text-brass/80">
            Mode Jeu
          </p>
          <h1 className="mt-1 font-display text-[28px] font-semibold leading-none tracking-tight text-paper">
            {active ? "Session active" : "Prêt à jouer"}
          </h1>
          <p className="mt-2 text-[13px] text-muted">
            {active
              ? `${session?.closed_names.length ?? 0} apps en pause · ${session!.freed_mb} Mo libérés`
              : `${closable} apps · ${reclaimable} Mo récupérables`}
          </p>
        </div>
        <div
          className={cn(
            "mt-1 flex h-9 items-center gap-2 rounded-full border px-3 text-[12px] font-semibold",
            active
              ? "border-jade/35 bg-jade/10 text-jade"
              : "border-line bg-surface/80 text-muted"
          )}
        >
          <span className={cn("h-1.5 w-1.5 rounded-full", active ? "bg-jade" : "bg-muted/60")} />
          {active ? "Actif" : "Veille"}
        </div>
      </header>

      <UpdateBar gameMode={active} />

      <nav className="relative z-10 mx-7 mb-5 flex gap-1 rounded-2xl border border-line bg-surface/70 p-1 backdrop-blur-sm">
        {TABS.map(({ id, label, icon: Icon }) => {
          const on = tab === id;
          return (
            <button
              key={id}
              type="button"
              onClick={() => setTab(id)}
              className={cn(
                "flex flex-1 items-center justify-center gap-2 rounded-xl px-3 py-2.5 text-[13px] font-semibold transition-colors",
                on
                  ? "bg-raised text-paper shadow-[inset_0_0_0_1px_rgba(214,166,74,0.28)]"
                  : "text-muted hover:text-paper"
              )}
            >
              <Icon className={cn("h-3.5 w-3.5", on ? "text-brass" : "")} />
              <span className="hidden sm:inline">{label}</span>
            </button>
          );
        })}
      </nav>

      <main className="scroll-area relative z-10 flex-1 overflow-y-auto px-7" style={{ paddingBottom: 132 }}>
        {tab === "session" && (
          <section className="animate-rise space-y-6">
            <div className="relative overflow-hidden rounded-[28px] border border-line bg-gradient-to-br from-raised via-surface to-ink p-7">
              <div className="pointer-events-none absolute -right-16 -top-20 h-56 w-56 rounded-full bg-brass/10 blur-3xl" />
              <div className="relative grid gap-8 sm:grid-cols-2">
                <Stat
                  value={active ? session!.freed_mb : reclaimable}
                  unit="Mo"
                  label={active ? "Mémoire libérée" : "Mémoire récupérable"}
                  accent
                />
                <Stat
                  value={active ? session!.closed_names.length : closable}
                  label={active ? "Applications fermées" : "Applications à fermer"}
                />
              </div>
            </div>

            <div className="grid gap-3 sm:grid-cols-3">
              <SummaryChip
                on={config.high_performance}
                title="Perf. élevées"
                detail="Plan d'alimentation"
              />
              <SummaryChip
                on={config.overlay.enabled}
                title="Overlay"
                detail={config.overlay.enabled ? "Actif en session" : "Désactivé"}
                onClick={() => setTab("overlay")}
              />
              <SummaryChip
                on={config.keep.length > 0}
                title={`${config.keep.length} gardées`}
                detail="Apps protégées"
                onClick={() => setTab("apps")}
              />
            </div>

            {sysInfo && (
              <div className="rounded-2xl border border-line bg-surface/60 p-5">
                <div className="flex flex-wrap items-start justify-between gap-3">
                  <div>
                    <p className="text-[11px] font-semibold uppercase tracking-[0.16em] text-muted">
                      Ta machine
                    </p>
                    <p className="mt-2 text-[13px] font-semibold text-paper">{sysInfo.cpu}</p>
                    <p className="mt-0.5 text-[13px] text-muted">
                      {sysInfo.gpu} · {sysInfo.ram_gb} Go RAM
                    </p>
                  </div>
                  <Button size="sm" variant="outline" onClick={applyAmdProfile}>
                    Profil FPS sûr
                  </Button>
                </div>
                <ul className="mt-4 space-y-1.5">
                  {sysInfo.tips.slice(0, 3).map((t) => (
                    <li key={t} className="text-[12px] leading-relaxed text-muted">
                      · {t}
                    </li>
                  ))}
                </ul>
              </div>
            )}

            <p className="max-w-[42ch] text-[13px] leading-relaxed text-muted">
              Lance le mode pour fermer le superflu. Les réglages fins sont dans les autres
              onglets — ici, l'essentiel.
            </p>
          </section>
        )}

        {tab === "apps" && (
          <section className="animate-rise space-y-4">
            <div className="flex items-end justify-between gap-4">
              <div>
                <h2 className="font-display text-lg font-semibold text-paper">Applications</h2>
                <p className="mt-1 text-[13px] text-muted">
                  Choisis ce qui reste ouvert. Les gardées apparaissent en premier.
                </p>
              </div>
              <span className="rounded-full border border-line bg-surface px-3 py-1 text-[12px] font-semibold text-muted">
                {config.keep.length} gardées
              </span>
            </div>

            <div className="flex flex-wrap gap-1.5">
              {KEEP_PRESETS.map((preset) => {
                const on = preset.keys.every((k) => config.keep.includes(k));
                const partial = !on && preset.keys.some((k) => config.keep.includes(k));
                return (
                  <button
                    key={preset.id}
                    type="button"
                    onClick={() => applyPreset(preset.keys, !on)}
                    className={cn(
                      "rounded-full border px-3 py-1.5 text-[12px] font-semibold transition-colors",
                      on
                        ? "border-brass/40 bg-brass/15 text-brass"
                        : partial
                          ? "border-jade/35 bg-jade/10 text-jade"
                          : "border-line bg-surface/70 text-muted hover:text-paper"
                    )}
                  >
                    {preset.label}
                  </button>
                );
              })}
            </div>

            <div className="flex flex-wrap items-center gap-2">
              <div className="relative min-w-[180px] flex-1">
                <Search className="pointer-events-none absolute left-4 top-1/2 h-4 w-4 -translate-y-1/2 text-muted" />
                <Input
                  value={query}
                  onChange={(e) => setQuery(e.target.value)}
                  placeholder="Rechercher"
                  className="pl-11"
                />
              </div>
              <div className="flex gap-1 rounded-xl border border-line bg-surface/70 p-1">
                {(
                  [
                    ["all", "Toutes"],
                    ["closable", "À fermer"],
                    ["kept", "Gardées"],
                  ] as const
                ).map(([id, label]) => (
                  <button
                    key={id}
                    type="button"
                    onClick={() => setAppFilter(id)}
                    className={cn(
                      "rounded-lg px-2.5 py-2 text-[12px] font-semibold transition-colors",
                      appFilter === id
                        ? "bg-raised text-paper shadow-[inset_0_0_0_1px_rgba(214,166,74,0.28)]"
                        : "text-muted hover:text-paper"
                    )}
                  >
                    {label}
                  </button>
                ))}
              </div>
              <div className="flex gap-1 rounded-xl border border-line bg-surface/70 p-1">
                {(
                  [
                    ["ram", "RAM"],
                    ["cpu", "CPU"],
                  ] as const
                ).map(([id, label]) => (
                  <button
                    key={id}
                    type="button"
                    onClick={() => setAppSort(id)}
                    className={cn(
                      "rounded-lg px-3 py-2 text-[12px] font-semibold transition-colors",
                      appSort === id
                        ? "bg-raised text-brass shadow-[inset_0_0_0_1px_rgba(214,166,74,0.28)]"
                        : "text-muted hover:text-paper"
                    )}
                  >
                    {label} ↓
                  </button>
                ))}
              </div>
            </div>

            {selected.size > 0 && (
              <div className="flex flex-wrap items-center gap-2 rounded-xl border border-brass/30 bg-brass/10 px-3 py-2">
                <span className="text-[12px] font-semibold text-brass">
                  {selected.size} sélectionnée{selected.size > 1 ? "s" : ""}
                </span>
                <Button size="sm" onClick={() => applyKeepSelection(true)}>
                  Garder
                </Button>
                <Button size="sm" variant="outline" onClick={() => applyKeepSelection(false)}>
                  Ne plus garder
                </Button>
                <button
                  type="button"
                  className="ml-auto text-[12px] text-muted hover:text-paper"
                  onClick={() => setSelected(new Set())}
                >
                  Annuler
                </button>
              </div>
            )}

            <div className="overflow-hidden rounded-2xl border border-line bg-surface/60">
              {visible.length === 0 ? (
                <p className="px-4 py-10 text-center text-[13px] text-muted">
                  Aucune application ne correspond.
                </p>
              ) : (
                visible.map((p, i) => {
                  const isSel = selected.has(p.key);
                  return (
                    <div
                      key={p.key}
                      className={cn(
                        "flex items-center gap-3 px-3 py-3.5 transition-colors hover:bg-raised/70",
                        i > 0 && "border-t border-line/80",
                        p.kept && "bg-brass/[0.04]",
                        isSel && "bg-brass/10"
                      )}
                    >
                      <button
                        type="button"
                        onClick={() => toggleSelect(p.key)}
                        className="shrink-0 text-muted hover:text-brass"
                        aria-label={isSel ? `Désélectionner ${p.name}` : `Sélectionner ${p.name}`}
                      >
                        {isSel ? (
                          <CheckSquare className="h-4 w-4 text-brass" />
                        ) : (
                          <Square className="h-4 w-4" />
                        )}
                      </button>
                      <button
                        type="button"
                        className="min-w-0 flex-1 text-left"
                        onClick={() => toggleKeep(p.key, !p.kept)}
                      >
                        <div className="flex items-center gap-2">
                          <span className="truncate text-sm font-medium text-paper">{p.name}</span>
                          {p.kept && (
                            <span className="rounded-md bg-brass/15 px-1.5 py-0.5 text-[10px] font-semibold uppercase tracking-wide text-brass">
                              Gardée
                            </span>
                          )}
                          {p.foreground && config.protect_foreground && (
                            <span className="inline-flex items-center gap-1 rounded-md bg-brass/10 px-1.5 py-0.5 text-[10px] font-semibold uppercase tracking-wide text-brass">
                              <Lock className="h-2.5 w-2.5" />
                              Focus
                            </span>
                          )}
                        </div>
                        <p className="mt-0.5 text-xs text-muted">
                          <span className={cn(appSort === "ram" && "text-paper/80")}>
                            {p.memory_mb} Mo
                          </span>
                          {" · "}
                          <span className={cn(appSort === "cpu" && "text-paper/80")}>
                            {Math.round(p.cpu_pct)}% CPU
                          </span>
                          {p.instances > 1 && ` · ${p.instances} processus`}
                          {!p.kept &&
                            p.memory_mb < (config.min_close_mb ?? 40) &&
                            " · sous seuil"}
                        </p>
                      </button>
                      <Switch
                        checked={p.kept}
                        onCheckedChange={(v) => toggleKeep(p.key, v)}
                        aria-label={`Garder ${p.name}`}
                      />
                    </div>
                  );
                })
              )}
            </div>
          </section>
        )}

        {tab === "overlay" && (
          <section className="animate-rise space-y-6">
            <div>
              <h2 className="font-display text-lg font-semibold text-paper">Overlay</h2>
              <p className="mt-1 max-w-[48ch] text-[13px] leading-relaxed text-muted">
                Panneau léger sur un écran, FPS lus sur l'écran de jeu. Une lecture par seconde
                environ.
              </p>
            </div>

            <div className="space-y-4 rounded-2xl border border-line bg-surface/60 p-5">
              <Setting
                label="Activer l'overlay"
                hint="Pendant le mode jeu, ou toujours si tu l'actives ci-dessous."
                checked={config.overlay.enabled}
                onChange={(v) => patchOverlay({ enabled: v, hidden: false })}
              />
              <Divider />
              <Setting
                label="Aussi hors mode jeu"
                hint="Le panneau reste visible en veille."
                checked={config.overlay.always}
                onChange={(v) => patchOverlay({ always: v })}
              />
            </div>

            <div className="rounded-2xl border border-line bg-surface/60 p-5">
              <MonitorPicker
                monitors={monitors}
                overlayName={config.overlay.monitor_name}
                gameName={config.overlay.game_monitor_name}
                mode={monitorMode}
                onModeChange={setMonitorMode}
                onSelect={(name) =>
                  patchOverlay(
                    monitorMode === "overlay"
                      ? { monitor_name: name }
                      : { game_monitor_name: name }
                  )
                }
              />
            </div>

            <div>
              <p className="mb-3 text-[12px] font-semibold uppercase tracking-[0.14em] text-muted">
                Métriques
              </p>
              <div className="grid grid-cols-2 gap-2 sm:grid-cols-3">
                {(
                  [
                    ["show_fps", "FPS", config.overlay.show_fps],
                    ["show_cpu", "CPU", config.overlay.show_cpu],
                    ["show_gpu", "GPU", config.overlay.show_gpu],
                    ["show_temps", "Temp.", config.overlay.show_temps],
                    ["show_ram", "RAM", config.overlay.show_ram],
                  ] as const
                ).map(([key, label, on]) => (
                  <button
                    key={key}
                    type="button"
                    onClick={() => patchOverlay({ [key]: !on })}
                    className={cn(
                      "rounded-xl border px-3 py-3 text-left text-[13px] font-semibold transition-colors",
                      on
                        ? "border-brass/35 bg-brass/12 text-brass"
                        : "border-line bg-surface text-muted hover:text-paper"
                    )}
                  >
                    {label}
                  </button>
                ))}
              </div>
            </div>

            <div className="grid gap-5 sm:grid-cols-2">
              <div className="rounded-2xl border border-line bg-surface/60 p-5">
                <p className="mb-3 text-[12px] font-semibold uppercase tracking-[0.14em] text-muted">
                  Coin
                </p>
                <div className="grid grid-cols-2 gap-2">
                  {(
                    [
                      ["top-left", "Haut G"],
                      ["top-right", "Haut D"],
                      ["bottom-left", "Bas G"],
                      ["bottom-right", "Bas D"],
                    ] as const
                  ).map(([value, label]) => (
                    <button
                      key={value}
                      type="button"
                      onClick={() => patchOverlay({ position: value })}
                      className={cn(
                        "rounded-xl border py-2.5 text-[12px] font-semibold transition-colors",
                        config.overlay.position === value
                          ? "border-brass/35 bg-brass/12 text-brass"
                          : "border-line bg-ink/40 text-muted hover:text-paper"
                      )}
                    >
                      {label}
                    </button>
                  ))}
                </div>
              </div>

              <div className="space-y-5 rounded-2xl border border-line bg-surface/60 p-5">
                <div>
                  <div className="mb-2 flex items-center justify-between">
                    <p className="text-[12px] font-semibold uppercase tracking-[0.14em] text-muted">
                      Opacité
                    </p>
                    <span className="font-mono text-[12px] text-paper">
                      {config.overlay.opacity}%
                    </span>
                  </div>
                  <input
                    type="range"
                    min={40}
                    max={100}
                    value={config.overlay.opacity}
                    onChange={(e) => patchOverlay({ opacity: Number(e.target.value) })}
                    className="w-full accent-[#D6A64A]"
                  />
                </div>
                <div>
                  <p className="mb-2 text-[12px] font-semibold uppercase tracking-[0.14em] text-muted">
                    Rafraîchissement
                  </p>
                  <div className="flex gap-2">
                    {[
                      [1000, "1s"],
                      [2000, "2s"],
                      [3000, "3s"],
                    ].map(([ms, label]) => (
                      <button
                        key={ms}
                        type="button"
                        onClick={() => patchOverlay({ interval_ms: Number(ms) })}
                        className={cn(
                          "flex-1 rounded-xl border py-2 text-[12px] font-semibold transition-colors",
                          config.overlay.interval_ms === ms
                            ? "border-brass/35 bg-brass/12 text-brass"
                            : "border-line bg-ink/40 text-muted hover:text-paper"
                        )}
                      >
                        {label}
                      </button>
                    ))}
                  </div>
                </div>
              </div>
            </div>
          </section>
        )}

        {tab === "options" && (
          <section className="animate-rise space-y-4">
            <div>
              <h2 className="font-display text-lg font-semibold text-paper">Options</h2>
              <p className="mt-1 text-[13px] text-muted">
                Tweaks appliqués à l’activation, restaurés à la sortie.
              </p>
            </div>
            <UpdatesPanel gameMode={active} />

            <div>
              <p className="mb-2 px-1 text-[11px] font-semibold uppercase tracking-[0.16em] text-muted">
                Session
              </p>
              <div className="space-y-0 overflow-hidden rounded-2xl border border-line bg-surface/60">
                <OptionRow
                  label="Protéger la fenêtre au premier plan"
                  hint="Le jeu déjà ouvert n'est jamais fermé."
                  checked={config.protect_foreground}
                  onChange={(v) => patch({ protect_foreground: v })}
                />
                <OptionRow
                  label="Réduire à l'activation"
                  hint="Mode Jeu passe en barre des tâches."
                  checked={config.minimize_on_activate}
                  onChange={(v) => patch({ minimize_on_activate: v })}
                />
                <OptionRow
                  label="Démarrer avec Windows"
                  hint="Lancement à la connexion."
                  checked={config.start_with_windows}
                  onChange={(v) => patch({ start_with_windows: v })}
                />
                <OptionRow
                  label="Rouvrir les apps à la sortie"
                  hint="Sinon seules les tweaks système sont restaurées."
                  checked={config.reopen_closed_apps}
                  onChange={(v) => patch({ reopen_closed_apps: v })}
                  last
                />
              </div>
            </div>

            <div>
              <p className="mb-2 px-1 text-[11px] font-semibold uppercase tracking-[0.16em] text-muted">
                Performances jeu
              </p>
              <div className="space-y-0 overflow-hidden rounded-2xl border border-line bg-surface/60">
                <OptionRow
                  label="Plan Performances élevées"
                  hint="Active le plan High Performance Windows."
                  checked={config.high_performance}
                  onChange={(v) =>
                    patch({
                      high_performance: v,
                      ultimate_performance: v ? config.ultimate_performance : false,
                    })
                  }
                />
                <OptionRow
                  label="Plan Ultimate Performance"
                  hint="Prioritaire si disponible (copie le schéma Windows)."
                  checked={config.ultimate_performance}
                  onChange={(v) =>
                    patch({
                      ultimate_performance: v,
                      high_performance: v ? true : config.high_performance,
                    })
                  }
                />
                <OptionRow
                  label="Mode Jeu Windows"
                  hint="Active Game Mode (priorité CPU/GPU au jeu)."
                  checked={config.enable_game_mode}
                  onChange={(v) => patch({ enable_game_mode: v })}
                />
                <OptionRow
                  label="Couper Game DVR / captures"
                  hint="Moins d’overhead Xbox Game Bar en arrière-plan."
                  checked={config.disable_game_dvr}
                  onChange={(v) => patch({ disable_game_dvr: v })}
                />
                <OptionRow
                  label="Couper les Widgets"
                  hint="Masque le panneau Widgets de la barre des tâches."
                  checked={config.disable_widgets}
                  onChange={(v) => patch({ disable_widgets: v })}
                />
                <OptionRow
                  label="Couper les apps en arrière-plan"
                  hint="Limite les UWP / apps Store en fond."
                  checked={config.disable_background_apps}
                  onChange={(v) => patch({ disable_background_apps: v })}
                />
                <OptionRow
                  label="MMCSS gaming"
                  hint="Plus de CPU/réseau pour le jeu (admin recommandé)."
                  checked={config.mmcss_gaming}
                  onChange={(v) => patch({ mmcss_gaming: v })}
                />
                <OptionRow
                  label="Couper les notifications toast"
                  hint="Évite les pop-ups pendant la session."
                  checked={config.disable_notifications}
                  onChange={(v) => patch({ disable_notifications: v })}
                />
                <OptionRow
                  label="Effets visuels → performances"
                  hint="Désactive animations / ombres Windows."
                  checked={config.visual_effects_perf}
                  onChange={(v) => patch({ visual_effects_perf: v })}
                />
                <OptionRow
                  label="Désactiver la transparence"
                  hint="Aero / acrylique off le temps de la session."
                  checked={config.disable_transparency}
                  onChange={(v) => patch({ disable_transparency: v })}
                />
                <OptionRow
                  label={`Services (${config.services.join(", ")})`}
                  hint="SysMain, WSearch, DiagTrack — admin recommandé."
                  checked={config.stop_services}
                  onChange={(v) => patch({ stop_services: v })}
                  last
                />
              </div>
              <div className="mt-3 rounded-2xl border border-line bg-surface/60 px-4 py-4">
                <div className="flex items-center justify-between gap-3">
                  <div>
                    <p className="text-[13px] font-semibold text-paper">Seuil de fermeture</p>
                    <p className="mt-1 text-[12px] text-muted">
                      Ignore les apps sous {config.min_close_mb ?? 40} Mo (plus rapide, moins de bruit).
                    </p>
                  </div>
                  <span className="font-mono text-[13px] text-brass">
                    {config.min_close_mb ?? 40} Mo
                  </span>
                </div>
                <input
                  type="range"
                  min={0}
                  max={200}
                  step={10}
                  value={config.min_close_mb ?? 40}
                  onChange={(e) => patch({ min_close_mb: Number(e.target.value) })}
                  className="mt-3 w-full accent-[#D6A64A]"
                />
              </div>
            </div>
          </section>
        )}
      </main>

      <div className="pointer-events-none absolute inset-x-0 bottom-0 z-20 h-36 bg-gradient-to-t from-ink via-ink/90 to-transparent" />
      <div className="absolute inset-x-0 bottom-0 z-30 px-7 pb-7">
        <Button
          size="lg"
          variant={active ? "outline" : "primary"}
          className="w-full"
          disabled={busy}
          onClick={() => run(active ? "restore" : "activate")}
        >
          {busy ? (
            <Loader2 className="h-5 w-5 animate-spin" />
          ) : active ? (
            <RotateCcw className="h-5 w-5" />
          ) : (
            <Gamepad2 className="h-5 w-5" />
          )}
          {active ? "Tout rouvrir" : "Activer le mode jeu"}
        </Button>
      </div>
    </div>
    </UpdateProvider>
  );
}

function Stat({
  value,
  unit,
  label,
  accent,
}: {
  value: number;
  unit?: string;
  label: string;
  accent?: boolean;
}) {
  return (
    <div>
      <div className="flex items-baseline gap-2">
        <span
          className={cn(
            "font-display text-[52px] font-semibold leading-none tracking-tighter",
            accent ? "text-brass" : "text-paper"
          )}
        >
          {value}
        </span>
        {unit && <span className="text-base font-medium text-muted">{unit}</span>}
      </div>
      <p className="mt-3 text-[13px] text-muted">{label}</p>
    </div>
  );
}

function SummaryChip({
  on,
  title,
  detail,
  onClick,
}: {
  on: boolean;
  title: string;
  detail: string;
  onClick?: () => void;
}) {
  const className = cn(
    "rounded-2xl border border-line bg-surface/70 px-4 py-3 text-left transition-colors",
    onClick && "hover:border-muted/40 hover:bg-raised"
  );
  const body = (
    <>
      <div className="flex items-center gap-2">
        <span className={cn("h-1.5 w-1.5 rounded-full", on ? "bg-jade" : "bg-muted/50")} />
        <p className="text-[13px] font-semibold text-paper">{title}</p>
      </div>
      <p className="mt-1 text-[12px] text-muted">{detail}</p>
    </>
  );
  if (onClick) {
    return (
      <button type="button" onClick={onClick} className={className}>
        {body}
      </button>
    );
  }
  return <div className={className}>{body}</div>;
}

function Setting({
  label,
  hint,
  checked,
  onChange,
}: {
  label: string;
  hint: string;
  checked: boolean;
  onChange: (v: boolean) => void;
}) {
  return (
    <div className="flex items-start justify-between gap-6">
      <div className="min-w-0">
        <p className="text-sm font-medium text-paper">{label}</p>
        <p className="mt-1 text-[13px] leading-relaxed text-muted">{hint}</p>
      </div>
      <Switch checked={checked} onCheckedChange={onChange} className="mt-0.5 shrink-0" />
    </div>
  );
}

function OptionRow({
  label,
  hint,
  checked,
  onChange,
  last,
}: {
  label: string;
  hint: string;
  checked: boolean;
  onChange: (v: boolean) => void;
  last?: boolean;
}) {
  return (
    <div
      className={cn(
        "flex items-start justify-between gap-6 px-5 py-4",
        !last && "border-b border-line/80"
      )}
    >
      <div className="min-w-0">
        <p className="text-sm font-medium text-paper">{label}</p>
        <p className="mt-1 text-[13px] leading-relaxed text-muted">{hint}</p>
      </div>
      <Switch checked={checked} onCheckedChange={onChange} className="mt-0.5 shrink-0" />
    </div>
  );
}

function Divider() {
  return <div className="h-px bg-line/80" />;
}
