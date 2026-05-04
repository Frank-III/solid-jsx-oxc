// Bug: OXC double-escapes literal HTML entities in JSX text. The source
// `&copy;` should be passed through to the SSR output as-is (it's already a
// valid HTML entity); OXC instead escapes the leading `&` and emits
// `&amp;copy;`, which the browser renders as the literal text "&copy;".

export default function App() {
  return (
    <span>&copy; 2026 Gothab</span>
  );
}
