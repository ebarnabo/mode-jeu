import { useEffect, useState } from "react";

export function PowerIgnition({
  open,
  freedMb,
  appsClosed,
  onDone,
}: {
  open: boolean;
  freedMb: number;
  appsClosed: number;
  onDone: () => void;
}) {
  const [displayMb, setDisplayMb] = useState(0);

  useEffect(() => {
    if (!open) {
      setDisplayMb(0);
      return;
    }

    const reduced =
      typeof window !== "undefined" &&
      window.matchMedia("(prefers-reduced-motion: reduce)").matches;

    if (reduced) {
      setDisplayMb(freedMb);
      const t = window.setTimeout(onDone, 700);
      return () => window.clearTimeout(t);
    }

    const duration = 900;
    const start = performance.now();
    let raf = 0;
    const tick = (now: number) => {
      const t = Math.min(1, (now - start) / duration);
      // ease-out-expo
      const e = t === 1 ? 1 : 1 - Math.pow(2, -10 * t);
      setDisplayMb(Math.round(freedMb * e));
      if (t < 1) raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);

    const end = window.setTimeout(onDone, 1600);
    return () => {
      cancelAnimationFrame(raf);
      window.clearTimeout(end);
    };
  }, [open, freedMb, onDone]);

  if (!open) return null;

  return (
    <div
      className="power-ignition pointer-events-none absolute inset-0 z-[60] overflow-hidden"
      role="status"
      aria-live="polite"
      aria-label={`Mode jeu engagé, ${freedMb} mégaoctets libérés`}
    >
      <div className="power-ignition__veil absolute inset-0" />
      <div className="power-ignition__burst absolute left-1/2 top-[42%] h-[120vmax] w-[120vmax] -translate-x-1/2 -translate-y-1/2" />
      <div className="power-ignition__ring absolute left-1/2 top-[42%] h-56 w-56 -translate-x-1/2 -translate-y-1/2" />
      <div className="power-ignition__ring power-ignition__ring--late absolute left-1/2 top-[42%] h-72 w-72 -translate-x-1/2 -translate-y-1/2" />

      <div className="absolute inset-x-0 top-[34%] flex flex-col items-center px-8 text-center">
        <p className="power-ignition__kicker text-[11px] font-semibold uppercase tracking-[0.28em] text-brass">
          Ignition
        </p>
        <p className="power-ignition__title mt-3 font-display text-[42px] font-semibold leading-none tracking-tight text-paper">
          Puissance engagée
        </p>
        <div className="power-ignition__meter mt-6 flex items-baseline gap-2">
          <span className="font-display text-[64px] font-semibold leading-none tracking-tighter text-brass tabular-nums">
            {displayMb}
          </span>
          <span className="text-lg font-semibold text-muted">Mo libres</span>
        </div>
        <p className="power-ignition__sub mt-3 text-[13px] text-muted">
          {appsClosed} application{appsClosed === 1 ? "" : "s"} en pause
        </p>
      </div>

      {/* Sparks — CSS only, contained */}
      <span className="power-spark power-spark--1" />
      <span className="power-spark power-spark--2" />
      <span className="power-spark power-spark--3" />
      <span className="power-spark power-spark--4" />
      <span className="power-spark power-spark--5" />
      <span className="power-spark power-spark--6" />
    </div>
  );
}
