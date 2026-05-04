export default function App() {
  const cls = "active";
  const href = () => "/explore";
  return (
    <a class={cls} href={href()}>Explore</a>
  );
}
