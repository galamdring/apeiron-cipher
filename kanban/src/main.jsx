import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import configPromise from "./api/config";

configPromise.then((config) => {
  ReactDOM.createRoot(document.getElementById("root")).render(
    <React.StrictMode>
      <App config={config} />
    </React.StrictMode>
  );
}).catch((e) => {
  document.getElementById("root").textContent = `Failed to load config: ${e.message}`;
});
