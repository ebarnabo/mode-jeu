import { useCallback, useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Gamepad2, Lock, Search, RotateCcw, Loader2, Shield } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Switch } from "@/components/ui/switch";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Badge } from "@/components/ui/badge";
import { cn } from "@/lib/utils";

type ProcGroup = {
  key: string;
  name: string;
  memory_mb: number;
  instances: number;
  path: string | null;
  protected: boolean;
  kept: boolean;
  foreground: boolean;
};

type Config = {
  keep: string[];
  high_performance: boolean;
  protect_foreground: boolean;
  stop_services: boolean;
  services: string[];
};

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
  const [query, setQuery] = useState("");
  const [busy, setBusy] = useState(false);

  const refresh = useCallback(async () => {
    const [c, s, p] = await Promise.all([
      invoke<Config>("get_config"),
      invoke<Session>("get_session"),
      invoke<ProcGroup[]>("list_processes"),
    ]);
    setConfig(c);
    setSession(s);
    setProcs(p);
  }, []);

  useEffect(() => {
    refresh();
    const id = setInterval(() => {
      if (!busy) invoke<ProcGroup[]>("list_processes").then(setProcs).catch(() => {});
    }, 5000);
    return () => clearInterval(id);
  }, [refresh, busy]);

  const patch = async (next: Partial<Config>) => {
    if (!config) return;
    const merged = { ...config, ...next };
    setConfig(merged);
    await invoke("save_config", { config: merged });
    setProcs(await invoke<ProcGroup[]>("list_processes"));
  };

  const toggleKeep = (key: string, keep: boolean) => {
    if (!config) return;
    patch({ keep: keep ? [...config.keep, key] : config.keep.filter((k) => k !== key) });
  };

  const run = async (cmd: "activate" | "restore") => {
    setBusy(true);
    try {
      setSession(await invoke<Session>(cmd));
      setProcs(await invoke<ProcGroup[]>("list_processes"));
    } finally {
      setBusy(false);
    }
  };

  const active = session?.active ?? false;

  const { closable, reclaimable } = useMemo(() => {
    const list = procs.filter(
      (p) => !p.protected && !p.kept && !(config?.protect_foreground && p.foreground)
    );
    return { closable: list.length, reclaimable: list.reduce((n, p) => n + p.memory_mb, 0) };
  }, [procs, config]);

  const visible = useMemo(() => {
    const q = query.trim().toLowerCase();
    return procs.filter((p) => !p.protected && (!q || p.name.toLowerCase().includes(q)));
  }, [procs, query]);

  if (!config) {
    return (
      <div className="flex h-full items-center justify-center">
        <Loader2 className="h-5 w-5 animate-spin text-muted" />
      </div>
    );
  }

  return (
    <div className="relative flex h-full flex-col overflow-hidden">
      <header
        data-tauri-drag-region
        className="flex items-center justify-between gap-4 px-8 pb-6 pt-8"
      >
        <div>
          <h1 className="text-[22px] font-extrabold tracking-tight">Mode Jeu</h1>
          <p className="mt-1 text-[13px] text-muted">
            {active
              ? `${session?.closed_names.length ?? 0} applications mises en pause`
              : "Prêt à libérer de la mémoire"}
          </p>
        </div>
        <div
          className={cn(
            "flex h-11 items-center justify-center gap-2 rounded-2xl border px-4 text-[13px] font-semibold transition-colors duration-300",
            active
              ? "border-jade/30 bg-jade/10 text-jade"
              : "border-line bg-surface text-muted"
          )}
        >
          <span className={cn("h-2 w-2 rounded-full", active ? "bg-jade" : "bg-muted/50")} />
          {active ? "Actif" : "En veille"}
        </div>
      </header>

      <main className="scroll-area flex-1 overflow-y-auto px-8" style={{ paddingBottom: 140 }}>
        <section className="animate-rise rounded-3xl border border-line bg-gradient-to-b from-raised to-surface p-8">
          <div className="flex flex-wrap items-end gap-x-16 gap-y-8">
            <div>
              <div className="flex items-baseline gap-2">
                <span className="text-[56px] font-extrabold leading-none tracking-tighter text-brass">
                  {active ? session!.freed_mb : reclaimable}
                </span>
                <span className="text-lg font-semibold text-muted">Mo</span>
              </div>
              <p className="mt-3 text-[13px] text-muted">
                {active ? "Mémoire libérée" : "Mémoire récupérable"}
              </p>
            </div>
            <div>
              <div className="text-[56px] font-extrabold leading-none tracking-tighter text-paper">
                {active ? session!.closed_names.length : closable}
              </div>
              <p className="mt-3 text-[13px] text-muted">
                {active ? "Applications fermées" : "Applications à fermer"}
              </p>
            </div>
          </div>
        </section>

        <Card className="mt-6">
          <CardHeader>
            <CardTitle>Réglages</CardTitle>
            <CardDescription>
              Ce qui se passe en plus de la fermeture des applications quand tu lances le mode jeu.
            </CardDescription>
          </CardHeader>
          <CardContent className="flex flex-col gap-6">
            <Setting
              label="Basculer sur le plan Performances élevées"
              hint="Le plan précédent est restauré à la sortie."
              checked={config.high_performance}
              onChange={(v) => patch({ high_performance: v })}
            />
            <Setting
              label="Protéger la fenêtre au premier plan"
              hint="Le jeu déjà lancé ne sera jamais fermé, même hors de ta liste."
              checked={config.protect_foreground}
              onChange={(v) => patch({ protect_foreground: v })}
            />
            <Setting
              label={`Arrêter les services ${config.services.join(", ")}`}
              hint="Nécessite de lancer Mode Jeu en administrateur."
              checked={config.stop_services}
              onChange={(v) => patch({ stop_services: v })}
            />
          </CardContent>
        </Card>

        <Card className="mt-6">
          <CardHeader>
            <div className="flex flex-wrap items-center justify-between gap-4">
              <div className="flex flex-col gap-2">
                <CardTitle>Applications ouvertes</CardTitle>
                <CardDescription>
                  Active l'interrupteur pour garder une application ouverte pendant le jeu.
                </CardDescription>
              </div>
              <Badge className="gap-1.5">
                <Shield className="h-3 w-3" />
                {config.keep.length} gardées
              </Badge>
            </div>
            <div className="relative mt-4">
              <Search className="pointer-events-none absolute left-4 top-1/2 h-4 w-4 -translate-y-1/2 text-muted" />
              <Input
                value={query}
                onChange={(e) => setQuery(e.target.value)}
                placeholder="Rechercher une application"
                className="pl-11"
              />
            </div>
          </CardHeader>
          <CardContent className="flex flex-col gap-2">
            {visible.length === 0 && (
              <p className="py-8 text-center text-[13px] text-muted">
                Aucune application ne correspond à cette recherche.
              </p>
            )}
            {visible.map((p) => (
              <div
                key={p.key}
                className="flex items-center gap-4 rounded-2xl px-4 py-3 transition-colors duration-200 hover:bg-raised"
              >
                <div className="min-w-0 flex-1">
                  <div className="flex items-center gap-2">
                    <span className="truncate text-sm font-medium">{p.name}</span>
                    {p.foreground && config.protect_foreground && (
                      <Badge className="border-brass/30 bg-brass/10 text-brass">
                        <Lock className="h-3 w-3" />
                        Au premier plan
                      </Badge>
                    )}
                  </div>
                  <p className="mt-0.5 text-xs text-muted">
                    {p.memory_mb} Mo
                    {p.instances > 1 && ` · ${p.instances} processus`}
                  </p>
                </div>
                <Switch
                  checked={p.kept}
                  onCheckedChange={(v) => toggleKeep(p.key, v)}
                  aria-label={`Garder ${p.name} ouvert`}
                />
              </div>
            ))}
          </CardContent>
        </Card>
      </main>

      <div className="pointer-events-none absolute inset-x-0 bottom-0 h-40 bg-gradient-to-t from-ink via-ink/95 to-transparent" />
      <div className="absolute inset-x-0 bottom-0 flex items-center gap-3 px-8 pb-8">
        <Button
          size="lg"
          variant={active ? "outline" : "primary"}
          className="flex-1"
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
  );
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
    <div className="flex items-start justify-between gap-8">
      <div className="min-w-0">
        <p className="text-sm font-medium">{label}</p>
        <p className="mt-1 text-[13px] leading-relaxed text-muted">{hint}</p>
      </div>
      <Switch checked={checked} onCheckedChange={onChange} className="mt-0.5 shrink-0" />
    </div>
  );
}
