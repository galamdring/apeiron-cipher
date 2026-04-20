import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import configPromise from "./api/config";
import { initColumns } from "./store/issues";
import { setProxyBase } from "./api/github";
import { setAuthProxyBase } from "./api/auth";

configPromise.then((config) => {
  if (!config.columns || !config.types) {
    document.getElementById("root").textContent =
      "Failed to load config: missing required 'columns' or 'types' keys in config.json";
    return;
  }
  if (!config.authProxyUrl) {
    document.getElementById("root").textContent =
      "Failed to load config: missing required 'authProxyUrl' key in config.json";
    return;
  }
  initColumns(config.columns, config.types, config.hiddenColumns);
  setProxyBase(config.authProxyUrl);
  setAuthProxyBase(config.authProxyUrl);
  ReactDOM.createRoot(document.getElementById("root")).render(
    <React.StrictMode>
      <App config={config} />
    </React.StrictMode>
  );
}).catch((e) => {
  document.getElementById("root").textContent = `Failed to load config: ${e.message}`;
});
