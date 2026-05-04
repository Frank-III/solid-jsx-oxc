function Btn(allProps: any) {
  return <button class="primary" {...allProps} />;
}

export default function App() {
  return <Btn type="button" data-foo="bar">click</Btn>;
}
