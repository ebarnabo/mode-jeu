import { useCallback, useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Loader2, Search, Trash2, Lock } from "lucide-react";
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

export function SoftwarePanel() {
  const [view, setView] = useState<View | null>(null);
  const [apps, setApps] = useState<InstalledApp[]>([]);
  const [query, setQuery] = useState("");
  const [loading, setLoading] = useState(false);
  const [analyzing, setAnalyzing] = useState(false);
  const [report, setReport] = useState<AnalyzeReport | null>(null);
  const [pending, setPending] = useState<InstalledApp | AdviceItem | null>(null);
  const [busyId, setBusyId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

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
    return apps.filter(
      (a) =>
        !q ||
        a.name.toLowerCase().includes(q) ||
        a.publisher.toLowerCase().includes(q)
    );
  }, [apps, query]);

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
      setError(
        typeof e === "string"
          ? e
          : "Le désinstalleur n’a pas démarré. Réessaie, ou lance Mode Jeu en administrateur."
      );
    } finally {
      setBusyId(null);
    }
  };

  return (
    <section className="animate-rise space-y-5">
      <div>
        <h2 className="font-display text-lg font-semibold text-paper">PC</h2>
        <p className="mt-1 max-w-[48ch] text-[13px] leading-relaxed text-muted">
          Rien ne tourne tant que tu n’as pas choisi une action.
        </p>
      </div>

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

      {view === null && (
        <p className="px-1 text-[13px] text-muted">
          Disques : santé SMART. Logiciels : désinstall propre. Classer : utile / à retirer / intouchable.
        </p>
      )}

      {error && (
        <p className="rounded-xl border border-[#E8A07A]/30 bg-[#E8A07A]/10 px-4 py-3 text-[13px] text-[#E8A07A]">
          {error}
        </p>
      )}

      {view === "disks" && <DisksPanel />}

      {view === "installed" && (
        <>
          <div className="relative">
            <Search className="pointer-events-none absolute left-4 top-1/2 h-4 w-4 -translate-y-1/2 text-muted" />
            <Input
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder="Nom ou éditeur"
              className="pl-11"
            />
          </div>
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
                    "flex items-center gap-3 px-4 py-3.5",
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
                    </div>
                    <p className="mt-0.5 truncate text-xs text-muted">
                      {a.publisher || "Éditeur inconnu"}
                      {a.version ? ` · ${a.version}` : ""}
                      {a.size_mb > 0 ? ` · ${a.size_mb} Mo` : ""}
                    </p>
                  </div>
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
                hint="Apps que tu utilises — à garder, ou à fermer seulement en mode jeu."
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
              Windows ouvre le désinstalleur officiel. Confirme dans cette fenêtre-là.
            </p>
            <div className="mt-6 flex flex-col gap-2">
              <Button disabled={!!busyId} onClick={() => void confirmUninstall()}>
                {busyId ? <Loader2 className="h-4 w-4 animate-spin" /> : <Trash2 className="h-4 w-4" />}
                Lancer le désinstalleur
              </Button>
              <button
                type="button"
                className="py-2 text-[13px] font-semibold text-muted hover:text-paper"
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
