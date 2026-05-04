import { For } from "solid-js";

export default function App() {
  const items = [
    { id: 1, name: "alpha", active: true },
    { id: 2, name: "beta", active: false },
  ];
  return (
    <ul class="list">
      <For each={items}>
        {(it) => (
          <li class={it.active ? "on" : "off"} data-id={it.id}>
            {it.name}
          </li>
        )}
      </For>
    </ul>
  );
}
