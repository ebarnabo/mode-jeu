import React from "react";
import ReactDOM from "react-dom/client";
import { getCurrentWindow } from "@tauri-apps/api/window";
import App from "./App";
import OverlayApp from "./OverlayApp";
import MixerFlyout from "./MixerFlyout";
import "./index.css";

function windowKind(): "overlay" | "mixer" | "main" {
  const q = new URLSearchParams(window.location.search).get("window");
  if (q === "overlay") return "overlay";
  if (q === "mixer") return "mixer";
  try {
    const label = getCurrentWindow().label;
    if (label === "overlay" || label.startsWith("notch-")) return "overlay";
    if (label === "mixer") return "mixer";
  } catch {
    /* web preview */
  }
  return "main";
}

const kind = windowKind();

if (kind === "overlay") {
  document.documentElement.classList.add("overlay-root");
  document.body.classList.add("overlay-body");
}
if (kind === "mixer") {
  document.documentElement.classList.add("mixer-root");
  document.body.classList.add("mixer-body");
}

const tree = kind === "overlay" ? <OverlayApp /> : kind === "mixer" ? <MixerFlyout /> : <App />;

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>{tree}</React.StrictMode>
);
