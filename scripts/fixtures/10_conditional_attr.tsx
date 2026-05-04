export default function App() {
  const isActive = () => [true, "page" as const];
  return (
    <a
      href="/x"
      aria-current={isActive()[1] ? "page" : undefined}
      class="link"
    >
      x
    </a>
  );
}
