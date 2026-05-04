import { Show } from "solid-js";

function Brand() {
  return <span class="brand">Gothab</span>;
}

export default function App() {
  const signedIn = () => true;
  const username = () => "bart";
  return (
    <header class="hdr">
      <Brand />
      <Show when={signedIn()} fallback={<a href="/login">Login</a>}>
        <span class="user">{username()}</span>
      </Show>
      <a href="/about">About</a>
    </header>
  );
}
