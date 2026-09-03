import { cn } from "@/lib/utils";

export type MonitorInfo = {
  name: string;
  x: number;
  y: number;
  width: number;
  height: number;
  is_primary: boolean;
  scale: number;
};

export function MonitorPicker({
  monitors,
  overlayName,
  gameName,
  mode,
  onModeChange,
  onSelect,
}: {
  monitors: MonitorInfo[];
  overlayName: string | null;
  gameName: string | null;
  mode: "overlay" | "game";
  onModeChange: (m: "overlay" | "game") => void;
  onSelect: (name: string) => void;
}) {
  if (monitors.length === 0) {
    return (
      <p className="py-6 text-center text-[13px] text-muted">Aucun écran détecté.</p>
    );
  }

  const minX = Math.min(...monitors.map((m) => m.x));
  const minY = Math.min(...monitors.map((m) => m.y));
  const maxX = Math.max(...monitors.map((m) => m.x + m.width));
  const maxY = Math.max(...monitors.map((m) => m.y + m.height));
  const worldW = Math.max(1, maxX - minX);
  const worldH = Math.max(1, maxY - minY);
  const viewW = 100;
  const viewH = Math.max(28, Math.round((worldH / worldW) * viewW));

  return (
    <div className="flex flex-col gap-4">
      <div className="flex flex-wrap gap-2">
        <ModeChip
          active={mode === "overlay"}
          label="Écran overlay"
          onClick={() => onModeChange("overlay")}
        />
        <ModeChip
          active={mode === "game"}
          label="Écran de jeu (FPS)"
          onClick={() => onModeChange("game")}
        />
      </div>
      <p className="text-[13px] text-muted">
        Clique un écran pour le définir comme{" "}
        {mode === "overlay" ? "overlay de perfs" : "source FPS"}.
      </p>
      <div className="overflow-hidden rounded-2xl border border-line bg-ink/60 p-4">
        <svg
          viewBox={`0 0 ${viewW} ${viewH}`}
          className="mx-auto h-auto w-full max-w-lg"
          role="img"
          aria-label="Disposition des écrans"
        >
          {monitors.map((m, idx) => {
            const x = ((m.x - minX) / worldW) * viewW;
            const y = ((m.y - minY) / worldH) * viewH;
            const w = (m.width / worldW) * viewW;
            const h = (m.height / worldH) * viewH;
            const isOverlay = overlayName === m.name;
            const isGame = gameName === m.name;
            const selected = mode === "overlay" ? isOverlay : isGame;
            return (
              <g key={m.name} className="cursor-pointer" onClick={() => onSelect(m.name)}>
                <rect
                  x={x + 0.6}
                  y={y + 0.6}
                  width={Math.max(4, w - 1.2)}
                  height={Math.max(4, h - 1.2)}
                  rx={1.4}
                  fill={selected ? "rgba(214,166,74,0.28)" : "#1D2027"}
                  stroke={selected ? "#D6A64A" : "#272B34"}
                  strokeWidth={0.35}
                />
                <text
                  x={x + w / 2}
                  y={y + h / 2 - 1.2}
                  textAnchor="middle"
                  fill="#F2F3F5"
                  style={{ fontSize: 3.2, fontWeight: 700 }}
                >
                  {m.is_primary ? "Principal" : `Écran ${idx + 1}`}
                </text>
                <text
                  x={x + w / 2}
                  y={y + h / 2 + 2.6}
                  textAnchor="middle"
                  fill="#8A8F9A"
                  style={{ fontSize: 2.4 }}
                >
                  {[isOverlay ? "Overlay" : null, isGame ? "Jeu" : null]
                    .filter(Boolean)
                    .join(" · ") || `${m.width}×${m.height}`}
                </text>
              </g>
            );
          })}
        </svg>
      </div>
      <div className="flex flex-wrap gap-3 text-[12px] text-muted">
        <Legend swatch="bg-brass/40 border-brass" label="Sélection active" />
        <Legend swatch="bg-raised border-line" label="Autre écran" />
      </div>
    </div>
  );
}

function ModeChip({
  active,
  label,
  onClick,
}: {
  active: boolean;
  label: string;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={cn(
        "rounded-xl border px-3 py-2 text-[13px] font-semibold transition-colors",
        active
          ? "border-brass/40 bg-brass/15 text-brass"
          : "border-line bg-surface text-muted hover:text-paper"
      )}
    >
      {label}
    </button>
  );
}

function Legend({ swatch, label }: { swatch: string; label: string }) {
  return (
    <span className="inline-flex items-center gap-2">
      <span className={cn("h-3 w-5 rounded border", swatch)} />
      {label}
    </span>
  );
}
