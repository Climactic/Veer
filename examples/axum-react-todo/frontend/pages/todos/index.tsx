import { Link, router, usePage } from "@inertiajs/react";
import Button from "../../components/Button";
import Layout from "../../components/Layout";

type Todo = { id: number; title: string; done: boolean };

export default function Index() {
  const { props } = usePage<{ todos: Todo[] }>();

  return (
    <Layout>
      <div
        style={{
          display: "flex",
          justifyContent: "space-between",
          alignItems: "center",
        }}
      >
        <h1>Todos</h1>
        <Link href="/todos/new" className="btn btn-primary">
          New todo
        </Link>
      </div>

      <ul className="todos">
        {props.todos.length === 0 ? (
          <li className="empty">No todos yet — create one to get started.</li>
        ) : (
          props.todos.map((t) => (
            <li key={t.id}>
              <span>{t.title}</span>
              <Button
                variant="danger"
                onClick={() => {
                  if (confirm(`Delete "${t.title}"?`)) {
                    router.delete(`/todos/${t.id}`);
                  }
                }}
              >
                Delete
              </Button>
            </li>
          ))
        )}
      </ul>
    </Layout>
  );
}
