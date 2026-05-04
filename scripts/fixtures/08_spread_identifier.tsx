function Btn(allProps: { class?: string; children?: any } & Record<string, any>) {
  const { class: cls, children, ...rest } = allProps;
  return (
    <button class={cls} {...rest}>{children}</button>
  );
}

export default function App() {
  return <Btn class="primary" type="button" data-foo="bar">click</Btn>;
}
