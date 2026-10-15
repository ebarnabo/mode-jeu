import { useCallback, useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Loader2, Search, Trash2, Lock, RefreshCw, FolderOpen, Copy, Check } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { DisksPanel } from "@/components/DisksPanel";
import { cn } from "@/lib/utils";

type InstalledApp = {
  id: string;
  name: string;
  publisher: string;
  version: string;
  size_mb: number;
  location: string | null;
  can_uninstall: boolean;
  locked: boolean;
  source: string;
  folder: string | null;
  duplicate_group: string;
  duplicate_with: string[];
};

type AdviceItem = {
  id: string;
  name: string;
  exe: string;
  publisher: string;
  reason: string;
  memory_mb: number;
  can_uninstall: boolean;
};

type AnalyzeReport = {
  prompt: string;
  scanned: number;
  urgent: AdviceItem[];
  useful: AdviceItem[];
  keep: AdviceItem[];
  locked: AdviceItem[];
};

type View = "disks" | "installed" | "open";
type SoftSort = "name" | "size" | "dup";

export function SoftwarePanel() {
  const [view, setView] = useState<View>("disks");
  const [apps, setApps] = useState<InstalledApp[]>([]);
  const [query, setQuery] = useState("");
  const [sort, setSort] = useState<SoftSort>("name");
  const [loading, setLoading] = useState(false);
  const [analyzing, setAnalyzing] = useState(false);
  const [report, setReport] = useState<AnalyzeReport | null>(null);
  const [pending, setPending] = useState<InstalledApp | AdviceItem | null>(null);
  const [busyId, setBusyId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);

  const refreshInstalled = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      setApps(await invoke<InstalledApp[]>("list_installed"));
    } catch {
      setError("Impossible de lire les logiciels installés.");
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    if (view === "installed") void refreshInstalled();
  }, [view, refreshInstalled]);

  const visible = useMemo(() => {
    const q = query.trim().toLowerCase();
    const list = apps.filter(
      (a) =>
        !q ||
        a.name.toLowerCase().includes(q) ||
        a.publisher.toLowerCase().includes(q) ||
        a.duplicate_group.toLowerCase().includes(q)
    );
    return [...list].sort((a, b) => {
      if (sort === "size") {
        return b.size_mb - a.size_mb || a.name.localeCompare(b.name, "fr");
      }
      if (sort === "dup") {
        const da = a.duplicate_group ? 0 : 1;
        const db = b.duplicate_group ? 0 : 1;
        return da - db || a.name.localeCompare(b.name, "fr");
      }
      return a.name.localeCompare(b.name, "fr");
    });
  }, [apps, query, sort]);

  const duplicateGroups = useMemo(() => {
    const groups = new Set<string>();
    for (const app of apps) {
      if (app.duplicate_group) groups.add(app.duplicate_group);
    }
    return groups.size;
  }, [apps]);

  const openView = (next: View) => {
    setView(next);
    if (next === "open" && !report) void runAnalyze();
  };

  const runAnalyze = async () => {
    setAnalyzing(true);
    setError(null);
    setView("open");
    try {
      setReport(await invoke<AnalyzeReport>("analyze_open_apps"));
    } catch {
      setError("Impossible de lire les apps ouvertes.");
    } finally {
      setAnalyzing(false);
    }
  };

  const confirmUninstall = async () => {
    if (!pending) return;
    setBusyId(pending.id);
    setError(null);
    try {
      await invoke("uninstall_app", { id: pending.id });
      setPending(null);
      await refreshInstalled();
    } catch (e) {
      const msg =
        typeof e === "string"
          ? e
          : e && typeof e === "object" && "message" in e
            ? String((e as { message: unknown }).message)
            : "Le désinstalleur n’a pas démarré. Réessaie, ou lance Mode Jeu en administrateur.";
      setError(msg);
      await refreshInstalled();
    } finally {
      setBusyId(null);
    }
  };

  const openFolder = async (app: InstalledApp) => {
    setError(null);
    try {
      await invoke("open_install_folder", { id: app.id });
    } catch (e) {
      setError(readError(e, "Impossible d’ouvrir le dossier."));
    }
  };

  const copyTriage = async () => {
    setError(null);
    try {
      await navigator.clipboard.writeText(triagePrompt(apps));
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1800);
    } catch {
      setError("La copie a échoué. Réessaie depuis la fenêtre Mode Jeu.");
    }
  };

  return (
    <section className="space-y-5">
      <div className="flex gap-1 rounded-2xl border border-line bg-surface/70 p-1">
        {(
          [
            ["disks", "Disques"],
            ["installed", "Logiciels"],
            ["open", "Classer"],
          ] as const
        ).map(([id, label]) => (
          <button
            key={id}
            type="button"
            onClick={() => openView(id)}
            className={cn(
              "flex-1 rounded-xl px-3 py-2.5 text-[13px] font-semibold transition-colors",
              view === id
                ? "bg-raised text-paper shadow-[inset_0_0_0_1px_rgba(214,166,74,0.28)]"
                : "text-muted hover:text-paper"
            )}
          >
            {label}
          </button>
        ))}
      </div>

      {error && (
        <p className="rounded-xl border border-[#E8A07A]/30 bg-[#E8A07A]/10 px-4 py-3 text-[13px] text-[#E8A07A]">
          {error}
        </p>
      )}

      {view === "disks" && <DisksPanel />}

      {view === "installed" && (
        <>
          <div className="flex flex-wrap gap-2">
            <div className="relative min-w-[160px] flex-1">
              <Search className="pointer-events-none absolute left-4 top-1/2 h-4 w-4 -translate-y-1/2 text-muted" />
              <Input
                value={query}
                onChange={(e) => setQuery(e.target.value)}
                placeholder="Nom ou éditeur"
                className="pl-11"
              />
            </div>
            <div className="flex gap-1 rounded-xl border border-line bg-surface/70 p-1">
              {(
                [
                  ["name", "Nom"],
                  ["size", "Taille"],
                  ["dup", "Doublons"],
                ] as const
              ).map(([id, label]) => (
                <button
                  key={id}
                  type="button"
                  onClick={() => setSort(id)}
                  className={cn(
                    "rounded-lg px-2.5 py-2 text-[12px] font-semibold transition-colors",
                    sort === id
                      ? "bg-raised text-paper shadow-[inset_0_0_0_1px_rgba(226,177,90,0.28)]"
                      : "text-muted hover:text-paper"
                  )}
                >
                  {label}
                  {id === "size" ? " ↓" : ""}
                </button>
              ))}
            </div>
            <Button
              size="sm"
              variant="outline"
              disabled={loading || apps.length === 0}
              onClick={() => void copyTriage()}
              className="shrink-0 border-brass/40 text-brass hover:border-brass/70"
            >
              {copied ? <Check className="h-3.5 w-3.5" /> : <Copy className="h-3.5 w-3.5" />}
              {copied ? "Copié" : "Copier le tri"}
            </Button>
            <Button
              size="sm"
              variant="outline"
              disabled={loading}
              onClick={() => void refreshInstalled()}
              aria-label="Actualiser"
            >
              <RefreshCw className={cn("h-3.5 w-3.5", loading && "animate-spin")} />
            </Button>
          </div>
          {!loading && apps.length > 0 && (
            <p className="px-1 text-[12px] leading-relaxed text-muted">
              {duplicateGroups === 0
                ? "Aucun doublon repéré. Le texte copié demande, pour chaque logiciel, s’il faut le garder ou le désinstaller."
                : `${duplicateGroups} groupe${duplicateGroups > 1 ? "s" : ""} de doublons. Le texte copié les signale : tu choisis laquelle garder.`}
            </p>
          )}
          <div className="overflow-hidden rounded-2xl border border-line bg-surface/60">
            {loading ? (
              <div className="flex justify-center py-12">
                <Loader2 className="h-5 w-5 animate-spin text-muted" />
              </div>
            ) : visible.length === 0 ? (
              <p className="px-4 py-10 text-center text-[13px] text-muted">Aucun logiciel.</p>
            ) : (
              visible.map((a, i) => (
                <div
                  key={a.id}
                  className={cn(
                    "list-row flex items-center gap-3 px-4 py-3.5",
                    i > 0 && "border-t border-line/80"
                  )}
                >
                  <div className="min-w-0 flex-1">
                    <div className="flex flex-wrap items-center gap-2">
                      <span className="truncate text-sm font-medium text-paper">{a.name}</span>
                      {a.locked && (
                        <span className="inline-flex items-center gap-1 rounded-md bg-raised px-1.5 py-0.5 text-[10px] font-semibold uppercase tracking-wide text-muted">
                          <Lock className="h-2.5 w-2.5" />
                          Protégé
                        </span>
                      )}
                      {a.duplicate_group && (
                        <span className="rounded-md bg-brass/15 px-1.5 py-0.5 text-[10px] font-semibold uppercase tracking-wide text-brass">
                          Doublon
                        </span>
                      )}
                    </div>
                    <p className="mt-0.5 truncate text-xs text-muted">
                      {a.publisher || "Éditeur inconnu"}
                      {a.version ? ` · ${a.version}` : ""}
                      {a.size_mb > 0 ? ` · ${a.size_mb} Mo` : ""}
                    </p>
                    {a.duplicate_with.length > 0 && (
                      <p className="mt-0.5 truncate text-[11px] text-brass/80">
                        Aussi : {a.duplicate_with.slice(0, 2).join(", ")}
                        {a.duplicate_with.length > 2 ? ` +${a.duplicate_with.length - 2}` : ""}
                      </p>
                    )}
                  </div>
                  <div className="flex shrink-0 items-center gap-1.5">
                  {a.folder && (
                    <Button
                      size="sm"
                      variant="outline"
                      aria-label={`Ouvrir le dossier de ${a.name}`}
                      title="Ouvrir le dossier d’installation"
                      onClick={() => void openFolder(a)}
                    >
                      <FolderOpen className="h-3.5 w-3.5" />
                    </Button>
                  )}
                  {a.can_uninstall ? (
                    <Button
                      size="sm"
                      variant="outline"
                      disabled={busyId === a.id}
                      onClick={() => setPending(a)}
                    >
                      {busyId === a.id ? (
                        <Loader2 className="h-3.5 w-3.5 animate-spin" />
                      ) : (
                        <Trash2 className="h-3.5 w-3.5" />
                      )}
                      Désinstaller
                    </Button>
                  ) : (
                    <span className="shrink-0 text-[11px] font-semibold text-muted">Impossible</span>
                  )}
                  </div>
                </div>
              ))
            )}
          </div>
        </>
      )}

      {view === "open" && (
        <div className="space-y-4">
          {!report && !analyzing && (
            <p className="px-1 text-[13px] text-muted">Lecture des process ouverts…</p>
          )}
          {analyzing && (
            <div className="flex justify-center py-12">
              <Loader2 className="h-5 w-5 animate-spin text-muted" />
            </div>
          )}
          {report && (
            <>
              <p className="text-[12px] text-muted">{report.scanned} processus lus.</p>
              <AdviceGroup
                title="Désinstaller d’urgence"
                hint="Bloat, essai antivirus, adware OEM."
                tone="urgent"
                items={report.urgent}
                onUninstall={setPending}
                busyId={busyId}
              />
              <AdviceGroup
                title="Utile"
                hint="À garder, ou à fermer seulement en mode jeu."
                tone="useful"
                items={report.useful}
                onUninstall={setPending}
                busyId={busyId}
              />
              <AdviceGroup
                title="Ne pas désinstaller"
                hint="Launchers, drivers, anti-cheat, sécurité."
                tone="keep"
                items={report.keep}
                onUninstall={setPending}
                busyId={busyId}
              />
              <AdviceGroup
                title="Impossible"
                hint="Windows, services, process protégés."
                tone="locked"
                items={report.locked}
                onUninstall={setPending}
                busyId={busyId}
              />
            </>
          )}
        </div>
      )}

      {pending && (
        <div className="fixed inset-0 z-50 flex items-end justify-center bg-ink/70 p-6 backdrop-blur-sm sm:items-center">
          <div className="w-full max-w-[400px] rounded-[28px] border border-line bg-gradient-to-br from-raised via-surface to-ink p-6">
            <h3 className="font-display text-[26px] font-semibold leading-tight tracking-tight text-paper">
              {pending.name}
            </h3>
            <p className="mt-3 text-[13px] leading-relaxed text-muted">
              {busyId
                ? "Termine dans la fenêtre Windows. On attend."
                : "Windows ouvre le désinstalleur officiel, avec UAC si besoin."}
            </p>
            <div className="mt-6 flex flex-col gap-2">
              <Button disabled={!!busyId} onClick={() => void confirmUninstall()}>
                {busyId ? <Loader2 className="h-4 w-4 animate-spin" /> : <Trash2 className="h-4 w-4" />}
                {busyId ? "Désinstallation en cours…" : "Lancer le désinstalleur"}
              </Button>
              <button
                type="button"
                className="py-2 text-[13px] font-semibold text-muted hover:text-paper disabled:opacity-40"
                disabled={!!busyId}
                onClick={() => setPending(null)}
              >
                Annuler
              </button>
            </div>
          </div>
        </div>
      )}
    </section>
  );
}

function readError(e: unknown, fallback: string) {
  if (typeof e === "string") return e;
  if (e && typeof e === "object" && "message" in e) return String((e as { message: unknown }).message);
  return fallback;
}

function triagePrompt(apps: InstalledApp[]) {
  const groups = new Map<string, string[]>();
  for (const app of apps) {
    if (!app.duplicate_group) continue;
    const names = groups.get(app.duplicate_group) ?? [];
    names.push(app.name);
    groups.set(app.duplicate_group, names);
  }
  const dupBlock =
    groups.size === 0
      ? "Aucun doublon détecté par l’app."
      : [...groups.entries()]
          .map(([group, names]) => `• ${group} : ${names.join(" ; ")}`)
          .join("\n");
  const lines = apps.map((app) => {
    const size = app.size_mb > 0 ? `${app.size_mb} Mo` : "taille inconnue";
    const folder = app.folder ?? "dossier inconnu";
    const dup = app.duplicate_group
      ? ` | DOUBLON « ${app.duplicate_group} » (${app.duplicate_with.length + 1} entrées)`
      : "";
    const locked = app.locked ? " | protégé, ne pas désinstaller" : "";
    return `- ${app.name} | ${app.publisher || "éditeur inconnu"} | ${app.version || "sans version"} | ${size} | ${app.source} | ${folder}${locked}${dup}`;
  });
  return `Tu tries les logiciels installés sur un PC Windows utilisé pour jouer. Pour CHAQUE logiciel, une seule action : Garder, Désinstaller, ou Doute.
But : enlever l’inutile (essais, bloat, barres d’outils, vieux utilitaires) sans casser Windows, les pilotes, Steam, Epic, les anti-cheat, ni les jeux.

Règles :
- Une ligne « protégé » ne se désinstalle pas.
- Les groupes DOUBLON sont le même logiciel en plusieurs entrées (32/64 bits, ou deux inscriptions). Dis pourquoi elles se recouvrent. Ne choisis pas laquelle garder : je trancherai.
- En cas de doute : Doute, pas Désinstaller.

Réponds en français, un tableau : Nom | Action | Pourquoi.

Doublons détectés :
${dupBlock}

Logiciels (${apps.length}) :
${lines.join("\n")}`;
}

function AdviceGroup({
  title,
  hint,
  tone,
  items,
  onUninstall,
  busyId,
}: {
  title: string;
  hint: string;
  tone: "urgent" | "useful" | "keep" | "locked";
  items: AdviceItem[];
  onUninstall: (item: AdviceItem) => void;
  busyId: string | null;
}) {
  const accent =
    tone === "urgent"
      ? "text-[#E8A07A]"
      : tone === "useful"
        ? "text-jade"
        : tone === "keep"
          ? "text-brass"
          : "text-muted";
  const dot =
    tone === "urgent"
      ? "bg-[#E8A07A]"
      : tone === "useful"
        ? "bg-jade"
        : tone === "keep"
          ? "bg-brass"
          : "bg-muted/50";

  return (
    <div className="overflow-hidden rounded-2xl border border-line bg-surface/60">
      <div className="flex items-start justify-between gap-3 border-b border-line/80 px-4 py-3">
        <div>
          <div className="flex items-center gap-2">
            <span className={cn("h-1.5 w-1.5 rounded-full", dot)} />
            <h3 className={cn("text-[13px] font-semibold", accent)}>{title}</h3>
          </div>
          <p className="mt-1 text-[12px] text-muted">{hint}</p>
        </div>
        <span className="font-mono text-[12px] text-muted">{items.length}</span>
      </div>
      {items.length === 0 ? (
        <p className="px-4 py-5 text-[13px] text-muted">Rien dans cette catégorie.</p>
      ) : (
        items.map((item, i) => (
          <div
            key={`${item.id}-${item.exe}`}
            className={cn("flex items-center gap-3 px-4 py-3", i > 0 && "border-t border-line/80")}
          >
            <div className="min-w-0 flex-1">
              <p className="truncate text-sm font-medium text-paper">{item.name}</p>
              <p className="mt-0.5 text-[12px] leading-relaxed text-muted">{item.reason}</p>
              <p className="mt-0.5 font-mono text-[11px] text-muted/80">
                {item.exe}
                {item.memory_mb > 0 ? ` · ${item.memory_mb} Mo` : ""}
              </p>
            </div>
            {item.can_uninstall && tone === "urgent" ? (
              <Button
                size="sm"
                variant="outline"
                disabled={busyId === item.id}
                onClick={() => onUninstall(item)}
              >
                Désinstaller
              </Button>
            ) : null}
          </div>
        ))
      )}
    </div>
  );
}
