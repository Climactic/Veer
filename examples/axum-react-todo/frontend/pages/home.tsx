import { Link } from "@inertiajs/react";
import Layout from "../components/Layout";

export default function Home() {
  return (
    <Layout>
      <h1>veer todo</h1>
      <p>
        A tiny end-to-end demo of <code>veer</code> driving a React + Vite
        frontend. Backend is axum on <code>:3000</code>; Vite handles assets and
        proxies XHRs through.
      </p>
      <p>
        <Link href="/todos">View todos →</Link>
      </p>
    </Layout>
  );
}
