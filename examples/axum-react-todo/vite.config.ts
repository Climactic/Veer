import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// CSR dev setup: Vite owns the browser at :5173 and proxies non-asset paths
// to the Rust backend at :3000. Browser navigations (Accept: text/html, no
// X-Inertia header) bypass the proxy and get index.html, so the SPA can boot
// on any URL refresh. Inertia XHRs (X-Inertia header) get proxied through to
// the backend and come back as page-object JSON.
export default defineConfig({
  plugins: [react()],
  server: {
    port: 5173,
    strictPort: true,
    proxy: {
      "^/(?!frontend|@vite|@react-refresh|@id|@fs|node_modules|src)": {
        target: "http://127.0.0.1:3000",
        changeOrigin: true,
        bypass: (req) => {
          const accept = req.headers.accept ?? "";
          const isInertia = req.headers["x-inertia"] === "true";
          if (accept.includes("text/html") && !isInertia) {
            return "/index.html";
          }
        },
      },
    },
  },
});
