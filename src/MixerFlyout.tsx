import { useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { VolumeMixer } from "@/components/VolumeMixer";

export default function MixerFlyout() {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        invoke("hide_mixer_flyout").catch(() => {});
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  return (
    <div className="h-full bg-[#121214] p-3">
      <VolumeMixer variant="flyout" />
    </div>
  );
}
