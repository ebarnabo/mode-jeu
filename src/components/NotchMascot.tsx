import { useEffect, useState } from "react";
import { cn } from "@/lib/utils";

const CALM_L =
  "M26.2 28.4C23.4 20.8 15.2 14.2 12.4 20.4C9.2 26.8 12.4 36 19.6 35.6C22.8 35.4 25.2 31.6 26.2 28.4Z";
const CALM_R =
  "M22.8 27.2C25.2 22.4 30.4 15.2 35 16.2C40.4 17.4 40.6 25.8 35.6 28.6C31.8 30.8 26.2 30.4 22.8 27.2Z";

type Mood = "calm" | "music" | "warn" | "hot" | "catch" | "hold";

const LABEL: Record<Mood, string> = {
  calm: "Tout est calme",
  music: "Musique en cours",
  warn: "Charge élevée",
  hot: "PC saturé",
  catch: "Prête à recevoir",
  hold: "Fichiers au chaud",
};

export function NotchMascot({
  cpu,
  ram,
  playing,
  holding,
  catching,
  track,
  onClick,
}: {
  cpu: number | null;
  ram: number | null;
  playing: boolean;
  holding: boolean;
  catching: boolean;
  track: string | null;
  onClick: () => void;
}) {
  const hot = (cpu ?? 0) >= 88 || (ram ?? 0) >= 92;
  const warn = !hot && ((cpu ?? 0) >= 72 || (ram ?? 0) >= 84);
  const mood: Mood = catching
    ? "catch"
    : hot
      ? "hot"
      : warn
        ? "warn"
        : playing
          ? "music"
          : holding
            ? "hold"
            : "calm";

  const [shifting, setShifting] = useState(false);
  useEffect(() => {
    setShifting(true);
    const t = window.setTimeout(() => setShifting(false), 340);
    return () => window.clearTimeout(t);
  }, [mood, playing ? track : ""]);

  const label = LABEL[mood];

  return (
    <button
      type="button"
      className={cn("mascot", shifting && "is-shifting")}
      data-mood={mood}
      data-playing={playing ? "1" : "0"}
      title={`${label} · Ouvrir Mode Jeu`}
      aria-label={`${label}. Ouvrir Mode Jeu`}
      onClick={onClick}
    >
      <span className="mascot__spin">
        <svg viewBox="0 0 48 48" aria-hidden="true">
          <circle className="mascot__disc" cx="24" cy="24" r="21.2" />
          <g className="mascot__glyph">
            <path className="mascot__l" d={CALM_L} />
            <path className="mascot__r" d={CALM_R} />
          </g>
        </svg>
      </span>
    </button>
  );
}
