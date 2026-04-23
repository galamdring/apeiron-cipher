import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import configPromise from "./api/config";
import { initColumns } from "./store/issues";

configPromise.then((config) => {
  if (!config.columns || !config.types) {
    document.getElementById("root").textContent =
      "Failed to load config: missing required 'columns' or 'types' keys in config.json";
    return;
  }
  initColumns(config.columns, config.types, config.hiddenColumns);
  ReactDOM.createRoot(document.getElementById("root")).render(
    <React.StrictMode>
      <App config={config} />
    </React.StrictMode>
  );
}).catch((e) => {
  document.getElementById("root").textContent = `Failed to load config: ${e.message}`;
});
