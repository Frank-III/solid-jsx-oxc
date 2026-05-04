// Bug: when `class` and `classList` coexist on the same element, OXC emits
// two separate `class="..."` attributes, while Babel merges them into one.
// Browsers keep only the first `class` attribute, so the second one's classes
// are lost in the SSR HTML — but DOM-side hydration adds them back at runtime.
// Mismatch → hydration breakage and visual flicker.

export default function App() {
  const isActive = () => true;
  return (
    <a
      href="/x"
      class="nav-link"
      classList={{ "is-active": isActive(), "nav-link-base": true }}
    >
      Explore
    </a>
  );
}
