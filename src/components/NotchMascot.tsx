import { useEffect, useRef, useState } from "react";
import { cn } from "@/lib/utils";

const BODY_L =
  "M20.2 28.2C15.4 26.6 10.6 29.6 11 34.4C11.4 39.2 16.6 41.2 21.4 38.6C24.2 37 24.8 32.4 23.2 29.8C22.4 28.6 21.4 28.2 20.2 28.2Z";
const BODY_R =
  "M27.6 27.6C32.6 25.8 38.4 28.6 37.8 33.8C37.2 39 32 41.2 27.2 38.4C24.4 36.8 23.8 32.2 25.4 29.4C26.2 28.2 26.8 27.8 27.6 27.6Z";

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
            <path className="mascot__l" d={BODY_L} />
            <path className="mascot__r" d={BODY_R} />
          </g>
          <g className="mascot__eyes">
            <g className="mascot__look">
              <g className="mascot__eye mascot__eye--l">
                <ellipse className="mascot__sclera" cx="15.4" cy="16.6" rx="3.85" ry="4.2" />
                <circle className="mascot__pupil" cx="15.8" cy="17.15" r="1.55" />
              </g>
              <g className="mascot__eye mascot__eye--r">
                <ellipse className="mascot__sclera" cx="32.6" cy="16.6" rx="3.85" ry="4.2" />
                <circle className="mascot__pupil" cx="33" cy="17.15" r="1.55" />
              </g>
            </g>
          </g>
        </svg>
      </span>
    </button>
  );
}
