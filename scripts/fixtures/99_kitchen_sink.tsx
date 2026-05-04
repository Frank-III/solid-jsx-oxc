// Kitchen-sink fixture: pulls together many patterns gothab uses, in a single
// renderable Solid component, so the diff harness produces a substantial
// HTML document that Babel and OXC can be compared on.
//
// Intentionally avoids const-folding-friendly literals: dynamic values come
// from function args / accessor functions so OXC and Babel both have to
// emit dynamic-text/marker code.

import { Show, For, Switch, Match } from "solid-js";

function Avatar(props: { url: string | undefined; name: string }) {
  return (
    <Show when={props.url} fallback={<span class="initial">{props.name.charAt(0)}</span>}>
      {(url) => <img src={url() as string} alt={props.name} class="avatar" />}
    </Show>
  );
}

function NavLink(props: { href: string; active?: boolean; children?: any }) {
  return (
    <a
      href={props.href}
      class="nav-link"
      classList={{ "is-active": !!props.active, "nav-link-base": true }}
    >
      {props.children}
    </a>
  );
}

function Item(props: { label: string; danger?: boolean; children?: any }) {
  return (
    <div class={props.danger ? "item danger" : "item"} role="menuitem">
      <span class="item-label">{props.label}</span>
      {props.children}
    </div>
  );
}

function StatusBadge(props: { status: "loading" | "ok" | "error" }) {
  return (
    <span class="badge">
      <Switch fallback={<em>?</em>}>
        <Match when={props.status === "loading"}>
          <em class="loading">Loading…</em>
        </Match>
        <Match when={props.status === "ok"}>
          <em class="ok">Ready</em>
        </Match>
        <Match when={props.status === "error"}>
          <em class="error">Failed</em>
        </Match>
      </Switch>
    </span>
  );
}

function Btn(allProps: any) {
  const { children, class: cls, ...rest } = allProps;
  return (
    <button {...rest} class={cls ?? "btn"}>
      {children}
    </button>
  );
}

export default function App() {
  // All dynamic values come from function calls so neither compiler can
  // const-fold them; this isolates marker-emission policy and structural
  // differences from the optimizer.
  const username = () => "bart";
  const avatar = () => "https://example.com/a.png" as string | undefined;
  const isSignedIn = () => true;
  const status = () => "loading" as const;
  const items = () => [
    { id: 1, name: "first", active: true },
    { id: 2, name: "second", active: false },
    { id: 3, name: "third", active: true },
  ];
  const greeting = (n: string) => `hi ${n}`;

  return (
    <div class="app" data-page="home">
      <header class="hdr">
        <a href="/" class="brand">
          <img src="/logo.png" alt="Gothab" />
          <span>Gothab</span>
        </a>

        <nav class="nav">
          <NavLink href="/explore" active>
            Explore
          </NavLink>
          <NavLink href="/about">About</NavLink>
          <NavLink href={"/u/" + username()}>{username()}</NavLink>
        </nav>

        <Show
          when={isSignedIn()}
          fallback={
            <div class="auth">
              <a href="/login">Login</a>
              <a href="/signup">Signup</a>
            </div>
          }
        >
          <div class="user-menu">
            <Avatar url={avatar()} name={username()} />
            <span class="user-name">Signed in as <strong>{username()}</strong></span>
            <StatusBadge status={status()} />
          </div>
        </Show>
      </header>

      <main class="main" style={{ padding: "1rem", "max-width": "960px" }}>
        <h1 class="title">{greeting(username())}</h1>

        <p class="lead">
          Welcome back, <strong>{username()}</strong>! Here's the latest:
        </p>

        <ul class="list">
          <For each={items()} fallback={<li class="empty">No items.</li>}>
            {(it, i) => (
              <li
                class="row"
                classList={{ "row-active": it.active, "row-odd": i() % 2 === 1 }}
                data-id={it.id}
              >
                <span class="row-name">{it.name}</span>
                <Btn type="button" data-id={it.id} class="row-btn">
                  edit
                </Btn>
              </li>
            )}
          </For>
        </ul>

        <section class="settings">
          <h2>Settings</h2>
          <Item label="Profile">
            <span class="hint">edit your details</span>
          </Item>
          <Item label="Sessions" />
          <Item label="Delete account" danger>
            <span class="hint">cannot be undone</span>
          </Item>
        </section>

        <svg viewBox="0 0 24 24" width="24" height="24" class="ic">
          <path d="M0 0 L24 24" stroke="currentColor" />
          <circle cx="12" cy="12" r="6" fill="none" stroke="currentColor" />
        </svg>

        <pre>
          {`line 1\nline 2 with <html> & "quotes"`}
        </pre>
      </main>

      <footer class="ftr">
        <span>&copy; {String(new Date().getFullYear())} Gothab</span>
      </footer>
    </div>
  );
}
