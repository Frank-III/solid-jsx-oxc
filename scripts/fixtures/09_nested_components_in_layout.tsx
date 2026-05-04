function Link(props: { href: string; children?: any }) {
  return <a href={props.href}>{props.children}</a>;
}

export default function App() {
  return (
    <div class="navInner">
      <div class="navLinks">
        <Link href="/explore">Explore</Link>
        <Link href="/about">About</Link>
      </div>
    </div>
  );
}
