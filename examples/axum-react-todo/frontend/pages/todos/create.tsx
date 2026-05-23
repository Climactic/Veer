import { Link, useForm } from "@inertiajs/react";
import Button from "../../components/Button";
import Field from "../../components/Field";
import Layout from "../../components/Layout";

export default function Create() {
  const form = useForm({ title: "" });

  return (
    <Layout>
      <h1>New todo</h1>
      <form
        onSubmit={(e) => {
          e.preventDefault();
          form.post("/todos");
        }}
      >
        <Field
          name="title"
          label="Title"
          autoFocus
          value={form.data.title}
          onChange={(e) => form.setData("title", e.target.value)}
          error={form.errors.title}
        />
        <div className="actions">
          <Button type="submit" disabled={form.processing}>
            {form.processing ? "Saving…" : "Create"}
          </Button>
          <Link href="/todos" className="btn btn-secondary">
            Cancel
          </Link>
        </div>
      </form>
    </Layout>
  );
}
