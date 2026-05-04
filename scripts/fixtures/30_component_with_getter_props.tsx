function Card(props: { title: string; children?: any }) {
  return (
    <div class="card">
      <h3>{props.title}</h3>
      <div class="body">{props.children}</div>
    </div>
  );
}

export default function App() {
  const title = () => "Hello";
  return (
    <Card title={title()}>
      <p>Some body</p>
    </Card>
  );
}
