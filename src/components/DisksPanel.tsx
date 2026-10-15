import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { HardDrive, Loader2, Sparkles, Wrench } from "lucide-react";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

type DiskVolume = {
  letter: string;
  label: string;
  fs: string;
  total_gb: number;
  free_gb: number;
  free_pct: number;
};

type PhysicalDisk = {
  index: number;
  model: string;
  bus: string;
  kind: string;
  status: "ok" | "watch" | "bad" | "critical" | string;
  status_label: string;
  advice: string[];
  temp_c: number | null;
  wear_pct: number | null;
  spare_pct: number | null;
  spare_threshold: number | null;
  power_on_hours: number | null;
  media_errors: number | null;
  predict_failure: boolean;
  trim: boolean;
  defrag_useful: boolean;
  optimize_action: string;
  source: string;
  volumes: DiskVolume[];
  frag_pct: number | null;
};

type DiskJob = {
  ok: boolean;
  letter: string;
  action: string;
  log: string;
};

export function DisksPanel() {
  const [disks, setDisks] = useState<PhysicalDisk[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [log, setLog] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      setDisks(await invoke<PhysicalDisk[]>("list_disks"));
    } catch {
      setError("Impossible de lire le SMART. Réessaie, ou lance l’app en administrateur.");
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const runJob = async (cmd: "optimize_volume" | "improve_volume" | "analyze_fragmentation", letter: string) => {
    setBusy(`${cmd}:${letter}`);
    setError(null);
    setLog(null);
    try {
      const job = await invoke<DiskJob>(cmd, { letter });
      setLog(job.log);
      setDisks(await invoke<PhysicalDisk[]>("list_disks"));
    } catch (e) {
      setError(typeof e === "string" ? e : "Action disque refusée. Essaie en administrateur.");
    } finally {
      setBusy(null);
    }
  };

  if (loading && disks.length === 0) {
    return (
      <div className="flex justify-center py-12">
        <Loader2 className="h-5 w-5 animate-spin text-muted" />
      </div>
    );
  }

  return (
    <div className="space-y-4">
      <p className="text-[13px] leading-relaxed text-muted">
        SMART firmware. SSD : TRIM. HDD : défrag si mécanique.
      </p>

      {error && (
        <p className="rounded-xl border border-[#E8A07A]/30 bg-[#E8A07A]/10 px-4 py-3 text-[13px] text-[#E8A07A]">
          {error}
        </p>
      )}

      {disks.length === 0 && (
        <p className="rounded-2xl border border-dashed border-line px-4 py-8 text-center text-[13px] text-muted">
          Aucun disque fixe lu. Relance l’app en administrateur si Windows bloque PhysicalDrive.
        </p>
      )}

      {disks.map((d) => (
        <article
          key={d.index}
          className="overflow-hidden rounded-[24px] border border-line bg-gradient-to-br from-raised via-surface to-ink"
        >
          <div className="p-5">
            <div className="flex items-start justify-between gap-3">
              <div className="flex min-w-0 items-start gap-3">
                <DiskStatusIcon status={d.status} kind={d.kind} label={d.status_label} />
                <div className="min-w-0">
                  <p className="text-[12px] text-muted">
                    {d.kind} · {d.bus} · {d.source}
                  </p>
                  <h3 className="mt-1 truncate font-display text-[20px] font-semibold tracking-tight text-paper">
                    {d.model}
                  </h3>
                </div>
              </div>
              <span
                className={cn(
                  "shrink-0 rounded-full border px-3 py-1 text-[12px] font-semibold",
                  d.status === "ok" && "border-jade/35 bg-jade/10 text-jade",
                  d.status === "watch" && "border-brass/35 bg-brass/10 text-brass",
                  (d.status === "bad" || d.status === "critical") &&
                    "border-[#E8A07A]/40 bg-[#E8A07A]/10 text-[#E8A07A]"
                )}
              >
                {d.status_label}
              </span>
            </div>

            <div className="mt-5 grid grid-cols-2 gap-3 sm:grid-cols-4">
              <Metric label="Temp" value={d.temp_c != null ? `${d.temp_c}°` : "—"} />
              <Metric
                label="Usure"
                value={d.wear_pct != null ? `${d.wear_pct}%` : "—"}
                hint="Percentage Used"
              />
              <Metric
                label="Spare"
                value={
                  d.spare_pct != null
                    ? `${d.spare_pct}%`
                    : "—"
                }
              />
              <Metric
                label="Heures"
                value={d.power_on_hours != null ? `${d.power_on_hours}` : "—"}
              />
            </div>

            <div className="mt-4 space-y-2">
              {d.volumes.map((v) => (
                <div key={v.letter} className="rounded-xl border border-line/80 bg-ink/40 px-3 py-2.5">
                  <div className="flex items-center justify-between gap-2 text-[13px]">
                    <span className="font-semibold text-paper">
                      {v.letter} {v.label && <span className="font-normal text-muted">{v.label}</span>}
                    </span>
                    <span className="font-mono text-[12px] text-muted">
                      {v.free_gb} / {v.total_gb} Go libres
                    </span>
                  </div>
                  <div className="mt-2 h-1.5 overflow-hidden rounded-full bg-line">
                    <div
                      className={cn(
                        "h-full rounded-full",
                        v.free_pct < 10 ? "bg-[#E8A07A]" : v.free_pct < 20 ? "bg-brass" : "bg-jade"
                      )}
                      style={{ width: `${Math.min(100, 100 - v.free_pct)}%` }}
                    />
                  </div>
                </div>
              ))}
            </div>

            {d.frag_pct != null && (
              <p className="mt-3 text-[12px] text-muted">Fragmentation mesurée : {d.frag_pct} %</p>
            )}

            <ul className="mt-4 space-y-1.5">
              {d.advice.map((a) => (
                <li key={a} className="text-[12px] leading-relaxed text-muted">
                  · {a}
                </li>
              ))}
            </ul>

            <div className="mt-5 flex flex-wrap gap-2">
              {d.volumes.map((v) => (
                <div key={v.letter} className="flex flex-wrap gap-2">
                  <Button
                    size="sm"
                    disabled={!!busy}
                    onClick={() => void runJob("optimize_volume", v.letter)}
                  >
                    {busy === `optimize_volume:${v.letter}` ? (
                      <Loader2 className="h-3.5 w-3.5 animate-spin" />
                    ) : (
                      <Sparkles className="h-3.5 w-3.5" />
                    )}
                    {d.optimize_action} {v.letter}
                  </Button>
                  <Button
                    size="sm"
                    variant="outline"
                    disabled={!!busy}
                    onClick={() => void runJob("improve_volume", v.letter)}
                  >
                    {busy === `improve_volume:${v.letter}` ? (
                      <Loader2 className="h-3.5 w-3.5 animate-spin" />
                    ) : (
                      <Wrench className="h-3.5 w-3.5" />
                    )}
                    Améliorer
                  </Button>
                  {d.defrag_useful && (
                    <Button
                      size="sm"
                      variant="ghost"
                      disabled={!!busy}
                      onClick={() => void runJob("analyze_fragmentation", v.letter)}
                    >
                      Frag. {v.letter}
                    </Button>
                  )}
                </div>
              ))}
            </div>
          </div>
        </article>
      ))}

      {log && (
        <pre className="max-h-40 overflow-auto rounded-2xl border border-line bg-ink/60 p-4 font-mono text-[11px] leading-relaxed text-muted">
          {log}
        </pre>
      )}
    </div>
  );
}

function DiskStatusIcon({
  status,
  kind,
  label,
}: {
  status: string;
  kind: string;
  label: string;
}) {
  const tone =
    status === "ok"
      ? "border-jade/35 bg-jade/12 text-jade"
      : status === "watch"
        ? "border-brass/35 bg-brass/12 text-brass"
        : "border-[#E8A07A]/40 bg-[#E8A07A]/12 text-[#E8A07A]";
  const flash = /ssd|nvme|flash/i.test(kind);
  return (
    <span
      title={`${kind} · ${label}`}
      className={cn(
        "relative flex h-11 w-11 shrink-0 items-center justify-center rounded-2xl border",
        tone
      )}
    >
      <HardDrive className="h-5 w-5" strokeWidth={1.75} />
      <span
        className={cn(
          "absolute -bottom-0.5 -right-0.5 h-2.5 w-2.5 rounded-full border-2 border-surface",
          status === "ok" && "bg-jade",
          status === "watch" && "bg-brass",
          (status === "bad" || status === "critical") && "bg-[#E8A07A]",
          !["ok", "watch", "bad", "critical"].includes(status) && "bg-muted"
        )}
      />
      {flash && (
        <span className="pointer-events-none absolute inset-x-2 top-1.5 h-px bg-current opacity-30" />
      )}
    </span>
  );
}

function Metric({ label, value, hint }: { label: string; value: string; hint?: string }) {
  return (
    <div>
      <p className="text-[11px] font-medium text-muted">{label}</p>
      <p className="mt-1 font-display text-[22px] font-semibold leading-none tracking-tight text-paper">
        {value}
      </p>
      {hint && <p className="mt-1 text-[10px] text-muted/80">{hint}</p>}
    </div>
  );
}

