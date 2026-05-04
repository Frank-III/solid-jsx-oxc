export default function App() {
  const html = () => "<b>bold</b>";
  return <div class="rich" innerHTML={html()} />;
}
