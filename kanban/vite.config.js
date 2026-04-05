import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  server: {
    proxy: {
      // Proxy GitHub OAuth endpoints to avoid CORS issues in the browser.
      // Only needed in development — in production you'd handle this server-side.
      "/github-oauth": {
        target: "https://github.com",
        changeOrigin: true,
        rewrite: (path) => path.replace(/^\/github-oauth/, ""),
      },
    },
  },
});
