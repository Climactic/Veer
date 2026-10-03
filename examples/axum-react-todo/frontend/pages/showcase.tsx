import { Deferred, InfiniteScroll, router, usePage } from "@inertiajs/react";
import Button from "../components/Button";
import Layout from "../components/Layout";
import { showcase, type Pages } from "../gen";

// Generated from `ShowcaseProps & ShowcaseClosureProps` (see src/todos.rs).
type Props = Extract<Pages, { component: "showcase" }>["props"];

export default function Showcase() {
  const { props } = usePage<Props>();

  return (
    <Layout>
      <h1>Showcase</h1>

      <h2>Big integer</h2>
      <p id="order">
        Order {String(props.orderId)} ({typeof props.orderId})
      </p>

      <h2>Once prop</h2>
      <p id="plans">Plans: {props.plans.join(", ")}</p>

      <h2>Deferred prop</h2>
      <Deferred data="stats" fallback={<p>Loading stats…</p>}>
        <p id="stats">{props.stats?.todos} todos in the store</p>
      </Deferred>

      <h2>Rescued prop</h2>
      <Deferred
        data="broken"
        fallback={<p>Loading…</p>}
        rescue={<p id="rescued">This prop failed on the server.</p>}
      >
        <p>Not shown: the prop always fails.</p>
      </Deferred>

      <h2>Fragment redirect</h2>
      <Button onClick={() => router.post(showcase.jump.url())}>
        Jump to the feed
      </Button>

      <h2 id="feed">Infinite scroll</h2>
      <InfiniteScroll data="feed">
        <ul className="todos">
          {props.feed.data.map((item) => (
            <li key={item.id}>{item.title}</li>
          ))}
        </ul>
      </InfiniteScroll>
    </Layout>
  );
}
