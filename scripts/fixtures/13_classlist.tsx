export default function App() {
  const isActive = () => true;
  const isMuted = () => false;
  return (
    <div
      classList={{
        "base": true,
        "active": isActive(),
        "muted": isMuted(),
      }}
    >
      x
    </div>
  );
}
