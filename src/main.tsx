import React from "react";
import ReactDOM from "react-dom/client";
import { getCurrentWindow } from "@tauri-apps/api/window";
import App from "./App";
import OverlayApp from "./OverlayApp";
import "./index.css";

function isOverlayWindow() {
  if (new URLSearchParams(window.location.search).get("window") === "overlay") return true;
  try {
    const label = getCurrentWindow().label;
    return label === "overlay" || label.startsWith("notch-");
  } catch {
    return false;
  }
}

const isOverlay = isOverlayWindow();

if (isOverlay) {
  document.documentElement.classList.add("overlay-root");
  document.body.classList.add("overlay-body");
}

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>{isOverlay ? <OverlayApp /> : <App />}</React.StrictMode>
);
