import { Show } from "solid-js";

export default function App() {
  const cond = () => true;
  const name = () => "bart";
  return (
    <div class="outer">
      <div class="inner">
        <Show when={cond()} fallback={<span>none</span>}>
          <span>{name()}</span>
        </Show>
      </div>
    </div>
  );
}
