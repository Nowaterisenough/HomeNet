import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import TrayStatus from "./components/TrayStatus";
import "./styles/global.css";
import "./styles/panels.css";
import "./styles/tray.css";

const isTray =
  new URLSearchParams(window.location.search).get("view") === "tray";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>{isTray ? <TrayStatus /> : <App />}</React.StrictMode>,
);
