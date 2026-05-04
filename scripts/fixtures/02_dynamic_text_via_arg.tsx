function greet(name: string) {
  return <div class="hello">Hi {name}!</div>;
}

export default function App() {
  return greet("bart");
}
