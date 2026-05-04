function Inner() {
  return <span>x</span>;
}

export default function App() {
  return (
    <div class="wrap">
      <Inner />
      <a href="/explore">Explore</a>
    </div>
  );
}
