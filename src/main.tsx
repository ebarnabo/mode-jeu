import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import OverlayApp from "./OverlayApp";
import "./index.css";

const isOverlay = new URLSearchParams(window.location.search).get("window") === "overlay";

if (isOverlay) {
  document.documentElement.classList.add("overlay-root");
  document.body.classList.add("overlay-body");
}

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>{isOverlay ? <OverlayApp /> : <App />}</React.StrictMode>
);
