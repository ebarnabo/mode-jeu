import { useEffect, useRef, useState } from "react";
import { cn } from "@/lib/utils";

const CALM_L =
  "M26.2 28.4C23.4 20.8 15.2 14.2 12.4 20.4C9.2 26.8 12.4 36 19.6 35.6C22.8 35.4 25.2 31.6 26.2 28.4Z";
const CALM_R =
  "M22.8 27.2C25.2 22.4 30.4 15.2 35 16.2C40.4 17.4 40.6 25.8 35.6 28.6C31.8 30.8 26.2 30.4 22.8 27.2Z";

type Mood = "calm" | "music" | "warn" | "hot" | "catch" | "hold";

const LABEL: Record<Mood, string> = {
  calm: "PC tranquille",
  music: "Musique en cours",
  warn: "Le PC monte en charge",
  hot: "PC saturé",
  catch: "Un fichier arrive",
  hold: "Des fichiers sont posés",
};

export function NotchMascot({
  cpu,
  ram,
  playing,
  holding,
  catching,
  track,
  compact,
  onClick,
}: {
  cpu: number | null;
  ram: number | null;
  playing: boolean;
  holding: boolean;
  catching: boolean;
  track: string | null;
  compact?: boolean;
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
  const mounted = useRef(false);
  useEffect(() => {
    if (!mounted.current) {
      mounted.current = true;
      return;
    }
    setShifting(true);
    const t = window.setTimeout(() => setShifting(false), 280);
    return () => window.clearTimeout(t);
  }, [mood, playing ? track : ""]);

  const label = LABEL[mood];

  return (
    <button
      type="button"
      className={cn("mascot", shifting && "is-shifting")}
      data-mood={mood}
      data-playing={playing ? "1" : "0"}
      title={compact ? `${label}. Cliquer pour agrandir.` : `Ouvrir Mode Jeu · ${label}`}
      aria-label={compact ? `${label}. Agrandir la notch.` : `Ouvrir Mode Jeu. ${label}.`}
      onClick={onClick}
    >
      <span className="mascot__spin">
        <svg viewBox="0 0 48 48" aria-hidden="true">
          <circle className="mascot__disc" cx="24" cy="24" r="21.2" />
          <g className="mascot__glyph">
            <path className="mascot__l" d={CALM_L} />
            <path className="mascot__r" d={CALM_R} />
          </g>
          <g className="mascot__eyes">
            <g className="mascot__look">
              <g className="mascot__eye mascot__eye--l">
                <ellipse className="mascot__sclera" cx="17.4" cy="16.1" rx="3.15" ry="3.45" />
                <circle className="mascot__pupil" cx="17.9" cy="16.6" r="1.32" />
              </g>
              <g className="mascot__eye mascot__eye--r">
                <ellipse className="mascot__sclera" cx="29.7" cy="15.4" rx="2.95" ry="3.2" />
                <circle className="mascot__pupil" cx="30.15" cy="15.9" r="1.24" />
              </g>
            </g>
          </g>
        </svg>
      </span>
    </button>
  );
}
