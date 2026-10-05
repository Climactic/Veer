import React from "react";
import { createRoot, hydrateRoot } from "react-dom/client";
import { createInertiaApp } from "@inertiajs/react";
import "./app.css";

// Two bootstrap paths share this file:
//
// - Rust-served shell (SSR or just script-tag rendering at :3000): the
//   Inertia v3 client reads the initial page from
//   `<script data-page="app" type="application/json">`, so we don't need to
//   pass `page:` ourselves. We hydrate vs mount fresh based on whether the
//   `#app` mount node already has SSR'd children.
//
// - Vite-served index.html (CSR-only mode at :5173): no script tag exists, so
//   we fetch the page object from the backend (proxied by Vite) and add the
//   same script tag. The client then parses it with its own parser, which
//   also revives big integers.
const pageScript = 'script[data-page="app"][type="application/json"]';

if (!document.querySelector(pageScript)) {
  const response = await fetch(
    window.location.pathname + window.location.search,
    { headers: { "X-Inertia": "true", "X-Inertia-Version": "dev" } },
  );
  const script = document.createElement("script");
  script.dataset.page = "app";
  script.type = "application/json";
  script.textContent = await response.text();
  document.body.appendChild(script);
}

createInertiaApp({
  resolve: (name) =>
    import.meta.glob<{ default: React.ComponentType }>("./pages/**/*.tsx", {
      eager: true,
    })[`./pages/${name}.tsx`].default,
  setup({ el, App, props }) {
    if (el.childNodes.length > 0) {
      hydrateRoot(el, <App {...props} />);
    } else {
      createRoot(el).render(<App {...props} />);
    }
  },
});
