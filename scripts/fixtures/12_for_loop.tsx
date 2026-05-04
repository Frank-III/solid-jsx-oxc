import { For } from "solid-js";

export default function App() {
  const items = ["a", "b", "c"];
  return (
    <ul class="list">
      <For each={items}>
        {(item) => <li>{item}</li>}
      </For>
    </ul>
  );
}
