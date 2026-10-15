import { useCallback, useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  ClipboardCopy,
  Gamepad2,
  Joystick,
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
  Plus,
  Package,
} from "lucide-react";
import { UpdateBar, UpdateProvider, UpdatesPanel } from "@/components/UpdateBar";
import { PowerIgnition } from "@/components/PowerIgnition";
import { SoftwarePanel } from "@/components/SoftwarePanel";
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
  notch: boolean;
  notch_position: string;
  notch_monitor_name: string | null;
  notch_all_screens?: boolean;
  notch_compact?: boolean;
  notch_show_ram?: boolean;
  notch_show_cpu?: boolean;
  notch_show_media?: boolean;
  notch_show_mascot?: boolean;
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
  cpu_unpark: boolean;
  cpu_boost: boolean;
  focus_assist: boolean;
  gpu_high_perf: boolean;
  min_close_mb: number;
  reopen_closed_apps: boolean;
  watch_games: boolean;
  auto_game_exes: string[];
  extra_games: { exe: string; name: string }[];
  music_launchers?: { name: string; path: string }[];
  overlay: OverlayConfig;
};

type GameEntry = {
  id: string;
  name: string;
  exe: string;
  source: string;
  path: string | null;
  running: boolean;
};

type GameDetected = {
  name: string;
  exe: string;
  source: string;
};

type Tab = "session" | "apps" | "jeux" | "pc" | "overlay" | "options";
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
  { id: "jeux", label: "Jeux", icon: Joystick },
  { id: "pc", label: "PC", icon: Package },
  { id: "overlay", label: "Overlay", icon: Monitor },
  { id: "options", label: "Options", icon: Settings2 },
];

type Session = {
  active: boolean;
  closed: string[];
  closed_names: string[];
  freed_mb: number;
  closed_use?: { name: string; mb: number; cpu_pct?: number }[];
};

export default function App() {
  const [config, setConfig] = useState<Config | null>(null);
  const [session, setSession] = useState<Session | null>(null);
  const [procs, setProcs] = useState<ProcGroup[]>([]);
  const [machineRamGb, setMachineRamGb] = useState<number | null>(null);
  const [usedRamGb, setUsedRamGb] = useState<number | null>(null);
  const [ramCopied, setRamCopied] = useState(false);
  const [monitors, setMonitors] = useState<MonitorInfo[]>([]);
  const [musicApps, setMusicApps] = useState<{ name: string; path: string; added: boolean }[]>([]);
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
  const [games, setGames] = useState<GameEntry[]>([]);
  const [gamesLoaded, setGamesLoaded] = useState(false);
  const [gamesBusy, setGamesBusy] = useState(false);
  const [gameQuery, setGameQuery] = useState("");
  const [prompt, setPrompt] = useState<GameDetected | null>(null);

  const refresh = useCallback(async () => {
    const [c, s] = await Promise.all([
      invoke<Config>("get_config"),
      invoke<Session>("get_session"),
    ]);
    setConfig(c);
    setSession(s);
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh]);

  useEffect(() => {
    if (busy || (tab !== "apps" && tab !== "session")) return;
    invoke<ProcGroup[]>("list_processes").then(setProcs).catch(() => {});
    invoke<{ ram_total_gb: number; ram_used_gb: number }>("get_metrics")
      .then((m) => {
        setMachineRamGb(m.ram_total_gb > 0 ? m.ram_total_gb : null);
        setUsedRamGb(m.ram_used_gb > 0 ? m.ram_used_gb : null);
      })
      .catch(() => {});
  }, [tab, busy]);

  useEffect(() => {
    if (tab !== "overlay") return;
    if (monitors.length === 0) {
      invoke<MonitorInfo[]>("list_monitors").then(setMonitors).catch(() => {});
    }
    invoke<{ name: string; path: string; added: boolean }[]>("list_music_apps")
      .then(setMusicApps)
      .catch(() => {});
  }, [tab, monitors.length]);

  useEffect(() => {
    let stop = false;
    const unsubs: Array<() => void> = [];
    listen<GameDetected>("game-detected", (e) => {
      if (!stop) setPrompt(e.payload);
    }).then((u) => {
      if (stop) u();
      else unsubs.push(u);
    });
    listen<Session>("session-changed", (e) => {
      if (!stop) {
        setSession(e.payload);
        if (e.payload.active) setPrompt(null);
      }
    }).then((u) => {
      if (stop) u();
      else unsubs.push(u);
    });
    return () => {
      stop = true;
      unsubs.forEach((u) => u());
    };
  }, []);

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

  const isAutoGame = (exe: string) =>
    (config?.auto_game_exes ?? []).includes(exe.toLowerCase());

  const setGameAuto = (exe: string, name: string, auto: boolean) => {
    if (!config) return;
    const key = exe.toLowerCase();
    const auto_game_exes = auto
      ? Array.from(new Set([...(config.auto_game_exes ?? []), key]))
      : (config.auto_game_exes ?? []).filter((e) => e !== key);
    const extra = config.extra_games ?? [];
    const extra_games = extra.some((g) => g.exe === key)
      ? extra
      : [...extra, { exe: key, name }];
    void patch({ auto_game_exes, extra_games });
  };

  const addOpenGame = (p: ProcGroup) => {
    if (!config) return;
    const key = p.key.toLowerCase();
    const extra = config.extra_games ?? [];
    if (extra.some((g) => g.exe === key)) return;
    const extra_games = [...extra, { exe: key, name: p.name }];
    void patch({ extra_games });
    setGames((prev) => {
      if (prev.some((g) => g.exe === key)) return prev;
      return [
        {
          id: `user:${key}`,
          name: p.name,
          exe: key,
          source: "Choisi",
          path: p.path,
          running: true,
        },
        ...prev,
      ];
    });
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

  const loadGames = async () => {
    setGamesBusy(true);
    try {
      if (config?.watch_games !== false) {
        await invoke("ensure_game_watch");
      }
      const [g, p] = await Promise.all([
        invoke<GameEntry[]>("list_games"),
        invoke<ProcGroup[]>("list_processes"),
      ]);
      setGames(g);
      setProcs(p);
      setGamesLoaded(true);
    } finally {
      setGamesBusy(false);
    }
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
      cpu_unpark: true,
      cpu_boost: true,
      focus_assist: true,
      gpu_high_perf: true,
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

  const visibleGames = useMemo(() => {
    const q = gameQuery.trim().toLowerCase();
    return games.filter(
      (g) =>
        !q ||
        g.name.toLowerCase().includes(q) ||
        g.exe.toLowerCase().includes(q) ||
        g.source.toLowerCase().includes(q)
    );
  }, [games, gameQuery]);

  const openCandidates = useMemo(() => {
    const known = new Set(games.map((g) => g.exe));
    return procs
      .filter(
        (p) =>
          !p.protected &&
          !known.has(p.key) &&
          p.memory_mb >= 40 &&
          !p.key.includes("steam") &&
          !p.key.includes("epic") &&
          p.key !== "explorer.exe"
      )
      .slice(0, 12);
  }, [procs, games]);

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

  const willClose = useMemo(() => {
    const minMb = config?.min_close_mb ?? 40;
    return procs
      .filter(
        (p) =>
          !p.protected &&
          !p.kept &&
          !(config?.protect_foreground && p.foreground) &&
          p.memory_mb >= minMb
      )
      .sort((a, b) => b.memory_mb - a.memory_mb);
  }, [procs, config]);

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
      {prompt && !active && (
        <div className="absolute inset-0 z-50 flex items-end justify-center bg-ink/70 p-6 backdrop-blur-sm sm:items-center">
          <div className="w-full max-w-[420px] overflow-hidden rounded-[28px] border border-brass/30 bg-gradient-to-br from-raised via-surface to-ink p-6 shadow-[0_24px_80px_rgba(0,0,0,0.55)]">
            <h2 className="font-display text-[32px] font-semibold leading-[0.95] tracking-tight text-paper">
              {prompt.name}
            </h2>
            <p className="mt-3 text-[13px] leading-relaxed text-muted">
              Ce jeu vient de s’ouvrir. Fermer le superflu et activer le mode jeu ?
            </p>
            <div className="mt-6 flex flex-col gap-2">
              <Button
                size="lg"
                className="w-full"
                disabled={busy}
                onClick={() => {
                  setPrompt(null);
                  void run("activate");
                }}
              >
                <Gamepad2 className="h-5 w-5" />
                Activer
              </Button>
              <Button
                size="lg"
                variant="outline"
                className="w-full"
                disabled={busy}
                onClick={() => {
                  setGameAuto(prompt.exe, prompt.name, true);
                  setPrompt(null);
                  void run("activate");
                }}
              >
                Toujours auto
              </Button>
              <button
                type="button"
                className="py-2 text-[13px] font-semibold text-muted hover:text-paper"
                onClick={() => setPrompt(null)}
              >
                Ignorer
              </button>
            </div>
          </div>
        </div>
      )}
      <header
        data-tauri-drag-region
        className="relative z-10 px-7 pb-4 pt-7"
      >
        <h1 className="font-display text-[28px] font-semibold leading-none tracking-tight text-paper">
          {active ? "Session active" : "Prêt à jouer"}
        </h1>
        {active && (
          <p className="mt-2 text-[13px] tabular-nums text-muted">
            {session?.closed_names.length ?? 0} apps en pause · {session!.freed_mb} Mo
          </p>
        )}
      </header>

      <UpdateBar gameMode={active} />

      <nav className="relative z-10 mx-7 mb-5 flex gap-1 rounded-2xl border border-line bg-surface/70 p-1">
        {TABS.map(({ id, label, icon: Icon }) => {
          const on = tab === id;
          return (
            <button
              key={id}
              type="button"
              onClick={() => setTab(id)}
              className={cn(
                "flex flex-1 items-center justify-center gap-1.5 rounded-xl px-2 py-2.5 text-[12px] font-semibold transition-colors",
                on
                  ? "bg-white/10 text-paper"
                  : "text-muted hover:text-paper"
              )}
            >
              <Icon className="h-3.5 w-3.5" />
              <span className="hidden sm:inline">{label}</span>
            </button>
          );
        })}
      </nav>

      <main className="scroll-area relative z-10 flex-1 overflow-y-auto px-7" style={{ paddingBottom: 132 }}>
        {tab === "session" && (
          <section className="space-y-4">
            {active ? (
              <ActivationBadges
                title={`${formatRam(session?.freed_mb ?? 0)} libérés`}
                note="Mesuré à la fermeture. Ce n’est pas un gain d’images."
                items={(session?.closed_use?.length
                  ? session.closed_use
                  : (session?.closed_names ?? []).map((name) => ({ name, mb: 0, cpu_pct: 0 }))
                ).map((item) => ({
                  name: item.name,
                  mb: item.mb,
                  cpu: item.cpu_pct ?? 0,
                }))}
              />
            ) : (
              <>
                <p className="max-w-[42ch] text-[15px] leading-relaxed text-muted">
                  Un bouton. Steam, Discord et Windows restent. Le reste se coupe le temps de jouer.
                </p>
                <ActivationBadges
                  title={
                    procs.length === 0
                      ? "Mesure…"
                      : willClose.length
                        ? `${formatRam(willClose.reduce((s, p) => s + p.memory_mb, 0))} occupés`
                        : "Rien à fermer"
                  }
                  note="RAM occupée maintenant par les apps qui seront fermées. Le CPU n’apparaît que s’il a été mesuré."
                  items={willClose.map((p) => ({
                    name: p.name,
                    mb: p.memory_mb,
                    cpu: p.cpu_pct,
                  }))}
                />
              </>
            )}
          </section>
        )}

        {tab === "apps" && (
          <section className="space-y-4">
            <RamBoard
              procs={procs}
              machineGb={machineRamGb}
              usedGb={usedRamGb}
              copied={ramCopied}
              onCopy={async () => {
                const text = ramPrompt(procs, machineRamGb, usedRamGb);
                try {
                  await navigator.clipboard.writeText(text);
                  setRamCopied(true);
                  window.setTimeout(() => setRamCopied(false), 1600);
                } catch {
                  setRamCopied(false);
                }
              }}
            />
            <div>
              <h2 className="font-display text-lg font-semibold text-paper">À garder</h2>
              <p className="mt-1 text-[13px] text-muted">Restent ouvertes en mode jeu.</p>
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
                        "list-row flex items-center gap-3 px-3 py-3.5 transition-colors hover:bg-raised/70",
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

        {tab === "jeux" && (
          <section className="space-y-5">
            <p className="max-w-[48ch] text-[13px] leading-relaxed text-muted">
              Auto = sans demander. Sinon, une proposition à l’ouverture.
            </p>

            <div className="overflow-hidden rounded-2xl border border-line bg-surface/60">
              <OptionRow
                label="Surveiller les lancements"
                hint="Off : rien en fond. On : demande ou auto à l’ouverture."
                checked={config.watch_games !== false}
                onChange={(v) => {
                  void patch({ watch_games: v });
                  if (v) void invoke("ensure_game_watch");
                }}
                last
              />
            </div>

            {!gamesLoaded ? (
              <div className="rounded-2xl border border-dashed border-line px-5 py-10 text-center">
                <p className="text-[13px] text-muted">Rien n’est lu tant que tu n’as pas cherché.</p>
                <Button className="mt-5" onClick={() => void loadGames()} disabled={gamesBusy}>
                  {gamesBusy ? <Loader2 className="h-4 w-4 animate-spin" /> : <Search className="h-4 w-4" />}
                  Chercher les jeux
                </Button>
              </div>
            ) : (
              <>
            <div className="relative">
              <Search className="pointer-events-none absolute left-4 top-1/2 h-4 w-4 -translate-y-1/2 text-muted" />
              <Input
                value={gameQuery}
                onChange={(e) => setGameQuery(e.target.value)}
                placeholder="Filtrer"
                className="pl-11"
              />
            </div>

            <div className="overflow-hidden rounded-2xl border border-line bg-surface/60">
              {visibleGames.length === 0 ? (
                <p className="px-4 py-10 text-center text-[13px] text-muted">
                  Aucun jeu. Installe Steam/Epic, ou ajoute un process ouvert.
                </p>
              ) : (
                visibleGames.map((g, i) => {
                  const auto = isAutoGame(g.exe);
                  return (
                    <div
                      key={g.id}
                      className={cn(
                        "list-row flex items-center gap-3 px-4 py-3.5",
                        i > 0 && "border-t border-line/80",
                        g.running && "bg-jade/[0.04]"
                      )}
                    >
                      <div className="min-w-0 flex-1">
                        <div className="flex flex-wrap items-center gap-2">
                          <span className="truncate text-sm font-medium text-paper">{g.name}</span>
                          {g.running && (
                            <span className="rounded-md bg-jade/15 px-1.5 py-0.5 text-[10px] font-semibold uppercase tracking-wide text-jade">
                              Ouvert
                            </span>
                          )}
                          <span className="rounded-md bg-raised px-1.5 py-0.5 text-[10px] font-semibold uppercase tracking-wide text-muted">
                            {g.source}
                          </span>
                        </div>
                        <p className="mt-0.5 font-mono text-[11px] text-muted">{g.exe}</p>
                      </div>
                      <div className="flex shrink-0 items-center gap-2">
                        <span className="text-[11px] font-semibold text-muted">Auto</span>
                        <Switch
                          checked={auto}
                          onCheckedChange={(v) => setGameAuto(g.exe, g.name, v)}
                          aria-label={`Mode jeu auto pour ${g.name}`}
                        />
                      </div>
                    </div>
                  );
                })
              )}
            </div>

            {openCandidates.length > 0 && (
              <div>
                <h3 className="mb-2 text-[13px] font-semibold text-paper">Déjà ouvert</h3>
                <p className="mb-3 text-[12px] text-muted">
                  Pas dans Steam / Epic : ajoute-le pour le reconnaître au prochain lancement.
                </p>
                <div className="flex flex-wrap gap-2">
                  {openCandidates.map((p) => (
                    <button
                      key={p.key}
                      type="button"
                      onClick={() => addOpenGame(p)}
                      className="inline-flex items-center gap-1.5 rounded-full border border-line bg-surface/80 px-3 py-1.5 text-[12px] font-semibold text-muted transition-colors hover:border-brass/40 hover:text-paper"
                    >
                      <Plus className="h-3 w-3 text-brass" />
                      {p.name}
                    </button>
                  ))}
                </div>
              </div>
            )}
              </>
            )}
          </section>
        )}

        {tab === "pc" && <SoftwarePanel />}

        {tab === "overlay" && (
          <section className="space-y-8">
            <div className="overflow-hidden rounded-2xl border border-line bg-surface/60">
              <OptionRow
                label="Montrer l’overlay"
                hint="Pendant le mode jeu, sauf si tu gardes hors session."
                checked={config.overlay.enabled}
                onChange={(v) => patchOverlay({ enabled: v, hidden: false })}
              />
              <OptionRow
                label="Garder hors session"
                hint="Le panneau reste à l’écran en veille."
                checked={config.overlay.always}
                onChange={(v) => patchOverlay({ always: v })}
              />
              <OptionRow
                label="Notch"
                hint="Capsule quand l’app est dans la barre des tâches. Masquée pendant un jeu."
                checked={config.overlay.notch !== false}
                onChange={(v) => patchOverlay({ notch: v })}
                last
              />
              {config.overlay.notch !== false && (
                <div className="space-y-4 border-t border-line/80 px-5 py-4">
                  <div>
                    <p className="mb-2 text-[13px] font-semibold text-paper">Position</p>
                    <div className="grid grid-cols-3 gap-2">
                      {(
                        [
                          ["top-left", "Haut G"],
                          ["top-center", "Haut C"],
                          ["top-right", "Haut D"],
                          ["bottom-left", "Bas G"],
                          ["bottom-center", "Bas C"],
                          ["bottom-right", "Bas D"],
                        ] as const
                      ).map(([value, label]) => (
                        <button
                          key={value}
                          type="button"
                          onClick={() => patchOverlay({ notch_position: value })}
                          className={cn(
                            "rounded-xl border py-2.5 text-[12px] font-semibold transition-colors",
                            (config.overlay.notch_position || "top-center") === value
                              ? "border-brass/35 bg-brass/12 text-brass"
                              : "border-line bg-ink/40 text-muted hover:text-paper"
                          )}
                        >
                          {label}
                        </button>
                      ))}
                    </div>
                  </div>
                  {monitors.length > 1 && (
                    <div>
                      <p className="mb-2 text-[13px] font-semibold text-paper">Écran</p>
                      <div className="flex flex-wrap gap-2">
                        <button
                          type="button"
                          onClick={() => patchOverlay({ notch_all_screens: true })}
                          className={cn(
                            "rounded-xl border px-3 py-2 text-[12px] font-semibold transition-colors",
                            config.overlay.notch_all_screens
                              ? "border-brass/35 bg-brass/12 text-brass"
                              : "border-line bg-ink/40 text-muted hover:text-paper"
                          )}
                        >
                          Tous
                        </button>
                        {monitors.map((m, i) => {
                          const selected =
                            !config.overlay.notch_all_screens &&
                            (config.overlay.notch_monitor_name
                              ? config.overlay.notch_monitor_name === m.name
                              : m.is_primary);
                          return (
                            <button
                              key={m.name}
                              type="button"
                              onClick={() =>
                                patchOverlay({
                                  notch_all_screens: false,
                                  notch_monitor_name: m.is_primary ? null : m.name,
                                })
                              }
                              className={cn(
                                "rounded-xl border px-3 py-2 text-[12px] font-semibold transition-colors",
                                selected
                                  ? "border-brass/35 bg-brass/12 text-brass"
                                  : "border-line bg-ink/40 text-muted hover:text-paper"
                              )}
                            >
                              {m.is_primary ? "Principal" : `Écran ${i + 1}`}
                            </button>
                          );
                        })}
                      </div>
                    </div>
                  )}
                  <div>
                    <p className="mb-2 text-[13px] font-semibold text-paper">Dans la notch</p>
                    <div className="flex flex-wrap gap-2">
                      {(
                        [
                          ["notch_show_ram", "RAM", config.overlay.notch_show_ram !== false],
                          ["notch_show_cpu", "CPU", config.overlay.notch_show_cpu !== false],
                          ["notch_show_media", "Lecture", config.overlay.notch_show_media !== false],
                          ["notch_show_mascot", "Avatar", config.overlay.notch_show_mascot !== false],
                        ] as const
                      ).map(([key, label, on]) => (
                        <button
                          key={key}
                          type="button"
                          onClick={() => patchOverlay({ [key]: !on })}
                          className={cn(
                            "rounded-xl border px-3 py-2 text-[12px] font-semibold transition-colors",
                            on
                              ? "border-brass/35 bg-brass/12 text-brass"
                              : "border-line bg-ink/40 text-muted hover:text-paper"
                          )}
                        >
                          {label}
                        </button>
                      ))}
                    </div>
                  </div>
                  <div>
                    <div className="mb-2 flex items-center justify-between gap-3">
                      <p className="text-[13px] font-semibold text-paper">Lecteurs</p>
                      <button
                        type="button"
                        className="rounded-lg border border-line px-2.5 py-1 text-[11px] font-semibold text-muted hover:text-paper"
                        onClick={() =>
                          invoke<{ name: string; path: string; added: boolean }[]>("pick_music_app")
                            .then(setMusicApps)
                            .catch(() => {})
                        }
                      >
                        Ajouter
                      </button>
                    </div>
                    {musicApps.length === 0 ? (
                      <p className="text-[12px] text-muted">
                        Aucun lecteur trouvé. Ajoute l’exécutable s’il n’est pas détecté.
                      </p>
                    ) : (
                      <ul className="space-y-1.5">
                        {musicApps.map((app) => (
                          <li
                            key={app.path}
                            className="flex items-center gap-2 rounded-xl border border-line bg-ink/40 px-3 py-2"
                          >
                            <span className="min-w-0 flex-1 truncate text-[12px] font-medium text-paper">
                              {app.name}
                            </span>
                            {app.added && (
                              <button
                                type="button"
                                className="text-[11px] font-semibold text-muted hover:text-paper"
                                onClick={() =>
                                  invoke<{ name: string; path: string; added: boolean }[]>(
                                    "remove_music_app",
                                    { path: app.path }
                                  )
                                    .then(setMusicApps)
                                    .catch(() => {})
                                }
                              >
                                Retirer
                              </button>
                            )}
                          </li>
                        ))}
                      </ul>
                    )}
                  </div>
                </div>
              )}
            </div>

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

            <div>
              <p className="mb-3 text-[13px] font-semibold text-paper">Métriques</p>
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

            <div className="grid gap-8 sm:grid-cols-2">
              <div>
                <p className="mb-3 text-[13px] font-semibold text-paper">Coin</p>
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

              <div className="space-y-5">
                <div>
                  <div className="mb-2 flex items-center justify-between">
                    <p className="text-[13px] font-semibold text-paper">Opacité</p>
                    <span className="font-mono text-[12px] tabular-nums text-paper">
                      {config.overlay.opacity}%
                    </span>
                  </div>
                  <input
                    type="range"
                    min={40}
                    max={100}
                    value={config.overlay.opacity}
                    onChange={(e) => patchOverlay({ opacity: Number(e.target.value) })}
                    className="w-full accent-[#E2B15A]"
                  />
                </div>
                <div>
                  <p className="mb-2 text-[13px] font-semibold text-paper">Rafraîchissement</p>
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
          <section className="space-y-4">
          <p className="text-[13px] text-muted">
            À l’activation, puis annulés à la sortie. Aucun badge n’annonce des images en plus.
          </p>
            <UpdatesPanel gameMode={active} />
            <div className="flex justify-end">
              <Button size="sm" variant="outline" onClick={applyAmdProfile}>
                Profil FPS sûr
              </Button>
            </div>

            <div>
              <p className="mb-2 px-1 text-[13px] font-semibold text-paper">Session</p>
              <div className="space-y-0 overflow-hidden rounded-2xl border border-line bg-surface/60">
                <OptionRow
                  label="Protéger la fenêtre au premier plan"
                  hint="Le jeu déjà ouvert n’est jamais fermé."
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
              <p className="mb-2 px-1 text-[13px] font-semibold text-paper">Performances</p>
              <div className="space-y-0 overflow-hidden rounded-2xl border border-line bg-surface/60">
                <OptionRow
                  label="Plan Performances élevées"
                  hint="Le plan High Performance de Windows."
                  badge="plan Windows"
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
                  hint="Utilisé seulement si Windows a ce schéma."
                  badge="si le schéma existe"
                  tone="limit"
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
                  hint="Le réglage Game Mode, pas un compteur d’images."
                  badge="réglage Windows"
                  checked={config.enable_game_mode}
                  onChange={(v) => patch({ enable_game_mode: v })}
                />
                <OptionRow
                  label="Couper Game DVR / captures"
                  hint="La barre Xbox n’enregistre plus en fond."
                  badge="captures off"
                  checked={config.disable_game_dvr}
                  onChange={(v) => patch({ disable_game_dvr: v })}
                />
                <OptionRow
                  label="Couper les Widgets"
                  hint="Le panneau de la barre des tâches."
                  badge="panneau masqué"
                  checked={config.disable_widgets}
                  onChange={(v) => patch({ disable_widgets: v })}
                />
                <OptionRow
                  label="Couper les apps en arrière-plan"
                  hint="Les apps du Store, pas tes jeux."
                  badge="apps Store"
                  checked={config.disable_background_apps}
                  onChange={(v) => patch({ disable_background_apps: v })}
                />
                <OptionRow
                  label="MMCSS gaming"
                  hint="Priorité CPU et réseau. Les images ne sont pas comptées."
                  badge="priorité, pas des FPS"
                  tone="limit"
                  checked={config.mmcss_gaming}
                  onChange={(v) => patch({ mmcss_gaming: v })}
                />
                <OptionRow
                  label="CPU à 100 % pendant la session"
                  hint="Plancher des cœurs, USB sans mise en veille."
                  badge="minimum 100 %"
                  checked={config.cpu_unpark !== false}
                  onChange={(v) => patch({ cpu_unpark: v })}
                />
                <OptionRow
                  label="Boost processeur agressif"
                  hint="Sur secteur, ça tient le plafond déjà atteint. Ça ne le relève pas."
                  badge="plafond tenu"
                  tone="limit"
                  checked={config.cpu_boost !== false}
                  onChange={(v) => patch({ cpu_boost: v })}
                />
                <OptionRow
                  label="Focus assist"
                  hint="Alarmes seulement, puis retour à l’état d’avant."
                  badge="alarmes seules"
                  checked={config.focus_assist !== false}
                  onChange={(v) => patch({ focus_assist: v })}
                />
                <OptionRow
                  label="GPU haute performance"
                  hint="Utile avec deux cartes. Avec une seule, Windows l’utilise déjà."
                  badge="rien à 1 GPU"
                  tone="limit"
                  checked={config.gpu_high_perf !== false}
                  onChange={(v) => patch({ gpu_high_perf: v })}
                />
                <OptionRow
                  label="Couper les notifications toast"
                  hint="Plus de pop-up pendant la session."
                  badge="pop-ups off"
                  checked={config.disable_notifications}
                  onChange={(v) => patch({ disable_notifications: v })}
                />
                <OptionRow
                  label="Effets visuels → performances"
                  hint="Animations et ombres de Windows."
                  badge="animations off"
                  checked={config.visual_effects_perf}
                  onChange={(v) => patch({ visual_effects_perf: v })}
                />
                <OptionRow
                  label="Désactiver la transparence"
                  hint="Acrylique coupé le temps de la session."
                  badge="acrylique off"
                  checked={config.disable_transparency}
                  onChange={(v) => patch({ disable_transparency: v })}
                />
                <OptionRow
                  label={`Services (${config.services.join(", ")})`}
                  hint="Ils s’arrêtent vraiment, puis repartent."
                  badge="arrêt puis retour"
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
                  className="mt-3 w-full accent-[#E2B15A]"
                />
              </div>
            </div>
          </section>
        )}
      </main>

      <div className="pointer-events-none absolute inset-x-0 bottom-0 z-20 h-36 bg-gradient-to-t from-ink via-ink/90 to-transparent" />
      <div className="absolute inset-x-0 bottom-0 z-30 px-7 pb-7">
        {!busy && (
          <p className="mb-2 text-center font-mono text-[12px] tabular-nums text-muted">
            {active
              ? `${formatRam(session?.freed_mb ?? 0)} libérés à la fermeture`
              : procs.length === 0
                ? "Mesure de la RAM…"
                : willClose.length
                ? `${formatRam(willClose.reduce((s, p) => s + p.memory_mb, 0))} occupés · ${willClose.length} apps`
                : "Aucune app au-dessus du seuil"}
          </p>
        )}
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

function OptionRow({
  label,
  hint,
  checked,
  onChange,
  last,
  badge,
  tone = "fact",
}: {
  label: string;
  hint: string;
  checked: boolean;
  onChange: (v: boolean) => void;
  last?: boolean;
  badge?: string;
  tone?: "fact" | "limit";
}) {
  return (
    <div
      className={cn(
        "flex items-start justify-between gap-4 px-5 py-4",
        !last && "border-b border-line/80"
      )}
    >
      <div className="min-w-0">
        <div className="flex flex-wrap items-center gap-2">
          <p className="text-sm font-medium text-paper">{label}</p>
          {badge && (
            <span
              className={cn(
                "rounded-full border px-2 py-0.5 text-[11px] font-medium",
                tone === "limit"
                  ? "border-line text-muted"
                  : "border-white/15 bg-white/5 text-paper"
              )}
            >
              {badge}
            </span>
          )}
        </div>
        <p className="mt-1 text-[13px] leading-relaxed text-muted">{hint}</p>
      </div>
      <Switch checked={checked} onCheckedChange={onChange} className="mt-0.5 shrink-0" />
    </div>
  );
}

function ActivationBadges({
  title,
  note,
  items,
}: {
  title: string;
  note: string;
  items: { name: string; mb: number; cpu: number }[];
}) {
  const shown = items.slice(0, 8);
  return (
    <div className="space-y-3">
      <p
        className={cn(
          "font-mono text-[28px] font-semibold leading-none tabular-nums",
          items.length ? "text-jade" : "text-paper"
        )}
      >
        {title}
      </p>
      {shown.length > 0 && (
        <div className="flex flex-wrap gap-1.5">
          {shown.map((item, i) => (
            <span
              key={`${item.name}-${i}`}
              className="inline-flex items-center gap-1.5 rounded-full border border-line bg-raised px-2.5 py-1 text-[12px] text-paper"
            >
              <span className="max-w-[16ch] truncate">{item.name}</span>
              {item.mb > 0 && (
                <span className="font-mono tabular-nums text-paper/80">{formatRam(item.mb)}</span>
              )}
              {item.cpu >= 1 && (
                <span className="font-mono tabular-nums text-muted">{Math.round(item.cpu)} % CPU</span>
              )}
            </span>
          ))}
          {items.length > shown.length && (
            <span className="rounded-full px-2 py-1 text-[12px] text-muted">
              +{items.length - shown.length}
            </span>
          )}
        </div>
      )}
    </div>
  );
}

function formatRam(mb: number) {
  if (mb >= 1024) {
    const go = mb / 1024;
    return `${go.toFixed(go >= 10 ? 0 : 1).replace(".", ",")} Go`;
  }
  return `${mb} Mo`;
}

function ramPrompt(apps: ProcGroup[], machineGb: number | null, usedGb: number | null) {
  const ranked = apps
    .filter((p) => !p.protected)
    .sort((a, b) => b.memory_mb - a.memory_mb);
  const sum = ranked.reduce((s, p) => s + p.memory_mb, 0);
  const lines = ranked.map((p) => {
    const flags = [
      p.kept ? "gardée en mode jeu" : "peut être fermée",
      p.foreground ? "au premier plan" : "en fond",
      p.instances > 1 ? `${p.instances} processus` : "1 processus",
    ].join(", ");
    return `- ${p.name} | ${p.memory_mb} Mo | CPU ${Math.round(p.cpu_pct)}% | ${flags}${p.path ? ` | ${p.path}` : ""}`;
  });
  const machine = machineGb ? `${machineGb.toFixed(1).replace(".", ",")} Go installés` : "taille inconnue";
  const used = usedGb ? `${usedGb.toFixed(1).replace(".", ",")} Go utilisés au total` : "usage total inconnu";
  return `Tu analyses la RAM d'un PC Windows utilisé pour jouer. Les applications ci-dessous sont OUVERTES maintenant. Ce n'est pas la liste des logiciels installés.

But : réduire la mémoire prise par ces apps ouvertes. Dis ce que je peux fermer, régler, ou désinstaller, sans casser Windows, Steam, Epic, les pilotes, les anti-cheat ni les jeux.

Pour chaque app, une seule action : Fermer maintenant, Régler, Désinstaller, ou Garder.
Pour Régler, donne le geste concret (onglets, démarrage, accélération matérielle, instances).
Ensuite, 5 actions classées de la plus utile à la moins utile.

Réponds en français. Tableau : Nom | Action | Mo récupérables | Pourquoi.

Machine : ${machine}, ${used}. Somme des apps listées : ${sum} Mo.

Apps ouvertes (${ranked.length}) :
${lines.join("\n")}`;
}

function RamBoard({
  procs,
  machineGb,
  usedGb,
  copied,
  onCopy,
}: {
  procs: ProcGroup[];
  machineGb: number | null;
  usedGb: number | null;
  copied: boolean;
  onCopy: () => void;
}) {
  const ranked = [...procs]
    .filter((p) => !p.protected)
    .sort((a, b) => b.memory_mb - a.memory_mb);
  const top = ranked[0];
  const sum = ranked.reduce((total, p) => total + p.memory_mb, 0);
  const machineMb = machineGb && machineGb > 0 ? machineGb * 1024 : null;
  const shown = ranked.slice(0, 6);
  const share = (mb: number) => (machineMb ? Math.round((mb / machineMb) * 100) : null);

  return (
    <div className="rounded-2xl border border-line bg-surface/70 p-5">
      <div className="flex items-start justify-between gap-4">
        <div className="min-w-0">
          {top ? (
            <>
              <p className="truncate font-display text-[28px] font-semibold leading-none tracking-tight text-paper">
                {top.name}
              </p>
              <p className="mt-2 font-mono text-[42px] font-semibold leading-none tabular-nums text-brass">
                {formatRam(top.memory_mb)}
              </p>
              <p className="mt-2 text-[13px] text-muted">
                {formatRam(sum)} dans les apps
                {machineMb ? ` · ${share(sum)}% de ${formatRam(Math.round(machineMb))}` : ""}
                {usedGb ? ` · ${formatRam(Math.round(usedGb * 1024))} utilisés` : ""}
              </p>
            </>
          ) : (
            <p className="mt-3 text-[14px] text-muted">Aucune app mesurée pour l’instant.</p>
          )}
        </div>
        <Button
          size="sm"
          variant="outline"
          disabled={!top}
          onClick={onCopy}
          className="shrink-0"
        >
          <ClipboardCopy className="h-3.5 w-3.5" />
          {copied ? "Copié" : "Copier l'analyse"}
        </Button>
      </div>
      {top && (
        <ul className="mt-5 space-y-3">
          {shown.map((p, i) => {
            const width = Math.max(4, Math.round((p.memory_mb / top.memory_mb) * 100));
            const pct = share(p.memory_mb);
            return (
              <li key={p.key}>
                <div className="flex items-baseline justify-between gap-3">
                  <span className="min-w-0 truncate text-[13px] font-medium text-paper">{p.name}</span>
                  <span className="shrink-0 font-mono text-[13px] font-semibold tabular-nums text-paper">
                    {formatRam(p.memory_mb)}
                    {pct != null && (
                      <span className="ml-2 font-medium text-muted">{pct === 0 ? "<1" : pct}%</span>
                    )}
                  </span>
                </div>
                <div className="mt-1.5 h-1.5 overflow-hidden rounded-full bg-ink">
                  <div
                    className={cn("h-full rounded-full", i === 0 ? "bg-brass" : "bg-paper/45")}
                    style={{ width: `${width}%` }}
                  />
                </div>
              </li>
            );
          })}
        </ul>
      )}
      {ranked.length > shown.length && (
        <p className="mt-3 text-[12px] text-muted">
          et {ranked.length - shown.length} autre{ranked.length - shown.length > 1 ? "s" : ""}, plus légères
        </p>
      )}
    </div>
  );
}
