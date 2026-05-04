export default function App() {
  const handle = () => console.log("hi");
  return (
    <button onClick={handle} class="btn">click</button>
  );
}
