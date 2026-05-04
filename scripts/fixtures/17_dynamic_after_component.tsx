function Icon() {
  return <i class="ic" />;
}

export default function App() {
  const label = () => "Click";
  return (
    <button>
      <Icon />
      <span>{label()}</span>
    </button>
  );
}
