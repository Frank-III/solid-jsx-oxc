function Btn(allProps: any) {
  const extra = { "aria-pressed": "true" };
  return <button {...allProps} {...extra} />;
}

export default function App() {
  return <Btn type="button" data-foo="bar">click</Btn>;
}
