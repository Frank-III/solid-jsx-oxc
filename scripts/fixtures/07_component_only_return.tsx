function Inner(props: { name: string }) {
  return <span>Hello, {props.name}</span>;
}

export default function App() {
  return <Inner name="bart" />;
}
