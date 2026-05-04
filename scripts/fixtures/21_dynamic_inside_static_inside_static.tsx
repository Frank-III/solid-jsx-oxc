function Inner(props: { name: string }) {
  return (
    <div class="navInner">
      <div class="navLinks">
        <a href={"/u/" + props.name}>{props.name}</a>
        <a href="/explore">Explore</a>
      </div>
    </div>
  );
}

export default function App() {
  return (
    <nav class="nav">
      <Inner name="bart" />
    </nav>
  );
}
