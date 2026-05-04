// Modeled after gothab's UserMenu/Dropdown shape: a Dropdown component with
// `trigger` and `children` children-getter slots, and inner Show/branches.

import { Show } from "solid-js";

function Dropdown(props: { trigger: any; children?: any }) {
  return (
    <div class="dd">
      <div class="dd-trigger">{props.trigger}</div>
      <div class="dd-menu">{props.children}</div>
    </div>
  );
}

function Item(props: { children?: any }) {
  return <div class="dd-item">{props.children}</div>;
}

export default function App() {
  const username = () => "bart";
  const avatar = () => "https://example.com/a.png";
  return (
    <Dropdown
      trigger={
        <button class="trig">
          <Show when={avatar()} fallback={<span>{username().charAt(0)}</span>}>
            {(url) => <img src={url() as string} alt={username()} />}
          </Show>
          <span>{username()}</span>
        </button>
      }
    >
      <div class="lbl">Signed in as <strong>{username()}</strong></div>
      <Item>Profile</Item>
      <Item>Settings</Item>
    </Dropdown>
  );
}
