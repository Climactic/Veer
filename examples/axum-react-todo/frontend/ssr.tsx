// SSR sidecar. Run with `bun frontend/ssr.tsx` (handled by the justfile when
// `SSR=1`). Listens on :13714/render; the Rust backend POSTs each page
// object here and inlines the returned head + body into the HTML shell.
//
// Pages are imported into a static map rather than via Vite's
// `import.meta.glob` — that's a Vite-specific transform Bun doesn't apply
// when running this file directly, and a 3-page demo doesn't need the
// dynamic version. For production, swap to `vite build --ssr frontend/ssr.tsx`
// and run the built bundle; the SSR adapter compiles the glob away anyway.

import createServer from "@inertiajs/react/server";
import { createInertiaApp } from "@inertiajs/react";
import ReactDOMServer from "react-dom/server";
import type { ComponentType } from "react";

import Home from "./pages/home";
import TodosIndex from "./pages/todos/index";
import TodosCreate from "./pages/todos/create";

const pages: Record<string, ComponentType<any>> = {
  home: Home,
  "todos/index": TodosIndex,
  "todos/create": TodosCreate,
};

createServer((page) =>
  createInertiaApp({
    page,
    render: ReactDOMServer.renderToString,
    resolve: (name) => {
      const c = pages[name];
      if (!c) throw new Error(`SSR: unknown component ${name}`);
      return { default: c };
    },
    setup: ({ App, props }) => <App {...props} />,
  }),
);
