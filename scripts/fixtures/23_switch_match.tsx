import { Switch, Match } from "solid-js";

export default function App() {
  const status = () => "loading" as "loading" | "ok" | "error";
  return (
    <div class="state">
      <Switch fallback={<span>?</span>}>
        <Match when={status() === "loading"}><span>Loading…</span></Match>
        <Match when={status() === "ok"}><span>Ready</span></Match>
        <Match when={status() === "error"}><span>Failed</span></Match>
      </Switch>
    </div>
  );
}
