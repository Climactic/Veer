import { Link, usePage } from "@inertiajs/react";
import type { ReactNode } from "react";

export default function Layout({ children }: { children: ReactNode }) {
  const success = usePage().flash.success as string | undefined;

  return (
    <div className="layout">
      <header className="layout-header">
        <div className="layout-header-inner">
          <Link href="/" className="brand">
            veer todo
          </Link>
          <nav className="layout-nav">
            <Link href="/">home</Link>
            <Link href="/todos">todos</Link>
            <Link href="/showcase">showcase</Link>
          </nav>
        </div>
      </header>
      <main className="layout-main">
        {success && <div className="flash flash-success">{success}</div>}
        {children}
      </main>
    </div>
  );
}
