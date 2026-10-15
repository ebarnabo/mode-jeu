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

  const lookRef = useRef<SVGGElement>(null);
  const eyeLRef = useRef<SVGGElement>(null);
  const eyeRRef = useRef<SVGGElement>(null);
  const glyphRef = useRef<SVGGElement>(null);

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

  useEffect(() => {
    if (window.matchMedia("(prefers-reduced-motion: reduce)").matches) return;
    let stop = false;
    let timer = 0;
    let kill: ((targets: Element[]) => void) | null = null;

    const targets = () =>
      [lookRef.current, eyeLRef.current, eyeRRef.current, glyphRef.current].filter(
        (node): node is SVGGElement => node != null
      );

    const sleep = (ms: number) =>
      new Promise<void>((resolve) => {
        timer = window.setTimeout(resolve, ms);
      });

    const parked = () =>
      stop || document.hidden || document.documentElement.dataset.quiet === "1";

    void (async () => {
      const { gsap } = await import("gsap");
      if (stop) return;
      gsap.ticker.lagSmoothing(500, 33);
      kill = (nodes) => gsap.killTweensOf(nodes);
      const still = { force3D: false, overwrite: "auto" as const };
      const y = mood === "hold" ? 1.05 : mood === "catch" || mood === "warn" ? -0.7 : 0;
      gsap.set(lookRef.current, { x: 0, y, ...still });
      gsap.set([eyeLRef.current, eyeRRef.current], {
        scaleY: mood === "warn" ? 0.48 : 1,
        ...still,
      });
      gsap.set(glyphRef.current, {
        scale: mood === "catch" ? 1.04 : 1,
        y: mood === "hold" ? 0.6 : 0,
        ...still,
      });

      const blink = async (twice = false) => {
        if (parked() || !eyeLRef.current || !eyeRRef.current) return;
        const shut = {
          scaleY: 0.08,
          duration: 0.055,
          yoyo: true,
          repeat: 1,
          ease: "power2.inOut",
          ...still,
        };
        const tl = gsap.timeline();
        tl.to(eyeLRef.current, shut, 0);
        tl.to(eyeRRef.current, shut, 0.06);
        if (twice) {
          tl.to(eyeLRef.current, shut, 0.2);
          tl.to(eyeRRef.current, shut, 0.26);
        }
        await tl;
      };

      const glance = async (x: number, yTo: number, duration = 0.16) => {
        if (parked() || !lookRef.current) return;
        await gsap.to(lookRef.current, { x, y: yTo, duration, ease: "power3.out", ...still });
      };

      while (!stop) {
        if (parked()) {
          gsap.killTweensOf(targets());
          await sleep(700);
          continue;
        }
        if (mood === "music" && glyphRef.current) {
          await gsap.to(glyphRef.current, { scale: 1.07, duration: 0.16, ease: "power2.out", ...still });
          if (!stop) {
            await gsap.to(glyphRef.current, { scale: 1, duration: 0.38, ease: "power3.out", ...still });
          }
          await glance(gsap.utils.random(-0.45, 0.45), -0.2, 0.18);
          await sleep(820);
        } else if (mood === "hot") {
          await glance(gsap.utils.random(-1.1, 1.1), gsap.utils.random(-0.4, 0.35), 0.1);
          if (Math.random() < 0.22) await blink();
          await sleep(280);
        } else if (mood === "warn") {
          await blink();
          await sleep(2400);
        } else if (mood === "catch") {
          await blink();
          await sleep(1500);
        } else if (mood === "hold") {
          await blink();
          await sleep(3400);
        } else {
          await blink(Math.random() < 0.34);
          await sleep(1700 + Math.random() * 1100);
          if (stop) break;
          await glance(Math.random() < 0.5 ? -1.05 : 1.05, Math.random() < 0.5 ? 0.2 : -0.4);
          await sleep(1500 + Math.random() * 900);
          if (!stop && !parked() && glyphRef.current) {
            await gsap.to(glyphRef.current, { scale: 1.045, duration: 0.5, ease: "power1.inOut", ...still });
            if (!stop) {
              await gsap.to(glyphRef.current, { scale: 1, duration: 0.72, ease: "power2.inOut", ...still });
            }
          }
          await sleep(2100);
        }
      }
    })();

    return () => {
      stop = true;
      window.clearTimeout(timer);
      kill?.(targets());
    };
  }, [mood]);

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
          <g className="mascot__glyph" ref={glyphRef}>
            <path className="mascot__l" d={BODY_L} />
            <path className="mascot__r" d={BODY_R} />
          </g>
          <g className="mascot__eyes">
            <g className="mascot__look" ref={lookRef}>
              <g className="mascot__eye mascot__eye--l" ref={eyeLRef}>
                <ellipse className="mascot__sclera" cx="15.4" cy="16.6" rx="3.85" ry="4.2" />
                <circle className="mascot__pupil" cx="15.8" cy="17.15" r="1.55" />
              </g>
              <g className="mascot__eye mascot__eye--r" ref={eyeRRef}>
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
