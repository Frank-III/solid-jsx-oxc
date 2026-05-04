// Bug: OXC drops the trailing whitespace from a JSX text fragment when it's
// followed by a JSX element. Babel preserves it.
//
//   "Welcome back, <strong>bart</strong>" → Babel emits "Welcome back, "
//   followed by <strong>; OXC emits "Welcome back," with no space.

export default function App() {
  const name = (n: string) => n;
  return (
    <p class="lead">
      Welcome back, <strong>{name("bart")}</strong>!
    </p>
  );
}
