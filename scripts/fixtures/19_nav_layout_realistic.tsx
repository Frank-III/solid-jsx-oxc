function A(props: { href: string; children?: any }) {
  return <a href={props.href}>{props.children}</a>;
}

export default function App() {
  return (
    <nav class="nav">
      <div class="navInner">
        <div class="navLinks">
          <A href="/explore">Explore</A>
        </div>
      </div>
    </nav>
  );
}
