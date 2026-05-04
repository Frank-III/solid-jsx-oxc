//! Comprehensive transform tests
//!
//! These tests verify the OXC compiler output matches expected SolidJS patterns.

use common::GenerateMode;
use solid_jsx_oxc::{transform, TransformOptions};

/// Helper to normalize whitespace for comparison
fn normalize(s: &str) -> String {
    s.lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Test helper that transforms and returns normalized code
fn transform_dom(source: &str) -> String {
    let result = transform(source, None);
    normalize(&result.code)
}

fn transform_ssr(source: &str) -> String {
    let options = TransformOptions {
        generate: GenerateMode::Ssr,
        ..TransformOptions::solid_defaults()
    };
    let result = transform(source, Some(options));
    normalize(&result.code)
}

// ============================================================================
// DOM: Basic Elements
// ============================================================================

#[test]
fn test_dom_static_element() {
    let code = transform_dom(r#"<div class="hello">world</div>"#);
    assert!(code.contains("template(`<div class=\"hello\">world</div>`)"));
    assert!(code.contains("cloneNode(true)"));
}

#[test]
fn test_dom_nested_elements() {
    let code = transform_dom(r#"<div><span>hello</span><p>world</p></div>"#);
    assert!(code.contains("template(`<div><span>hello</span><p>world</p></div>`)"));
}

#[test]
fn test_dom_void_element() {
    let code = transform_dom(r#"<input type="text" />"#);
    assert!(code.contains("template(`<input type=\"text\">`)"));
    // Void elements don't have closing tags
    assert!(!code.contains("</input>"));
}

#[test]
fn test_dom_self_closing() {
    let code = transform_dom(r#"<div />"#);
    assert!(code.contains("template(`<div></div>`)"));
}

// ============================================================================
// DOM: Dynamic Attributes
// ============================================================================

#[test]
fn test_dom_dynamic_class() {
    let code = transform_dom(r#"<div class={style()}>content</div>"#);
    assert!(code.contains("effect"));
    assert!(code.contains("setAttribute"));
    assert!(code.contains("style()"));
}

#[test]
fn test_dom_dynamic_multiple_attrs() {
    let code = transform_dom(r#"<div class={cls()} id={id()}>content</div>"#);
    assert!(code.contains("cls()"));
    assert!(code.contains("id()"));
}

#[test]
fn test_dom_mixed_static_dynamic() {
    let code = transform_dom(r#"<div class="static" id={dynamic()}>content</div>"#);
    // Static class should be in template
    assert!(code.contains("class=\"static\""));
    // Dynamic id should use effect
    assert!(code.contains("dynamic()"));
}

#[test]
fn test_dom_boolean_attribute() {
    let code = transform_dom(r#"<input disabled />"#);
    assert!(code.contains("disabled"));
}

#[test]
fn test_dom_class_namespace_binding() {
    let code = transform_dom(r#"<div class:my-class={props.active} />"#);
    assert!(code.contains("classList.toggle"));
    assert!(code.contains("\"my-class\""));
    assert!(code.contains("props.active"));
}

#[test]
fn test_dom_style_namespace_binding() {
    let code = transform_dom(r#"<div style:padding-top={props.top} />"#);
    assert!(code.contains("setStyleProperty"));
    assert!(code.contains("\"padding-top\""));
    assert!(code.contains("props.top"));
}

// ============================================================================
// DOM: Event Handlers
// ============================================================================

#[test]
fn test_dom_onclick_delegated() {
    let code = transform_dom(r#"<button onClick={handler}>click</button>"#);
    // Delegated events use $$eventName
    assert!(code.contains("$$click"));
    assert!(code.contains("delegateEvents"));
}

#[test]
fn test_dom_oncapture_not_delegated() {
    let code = transform_dom(r#"<button onClickCapture={handler}>click</button>"#);
    // Capture events are not delegated
    assert!(code.contains("addEventListener"));
}

#[test]
fn test_dom_onscroll_not_delegated() {
    let code = transform_dom(r#"<div onScroll={handler}>scroll</div>"#);
    // scroll is not delegated by default
    assert!(code.contains("addEventListener") || code.contains("onscroll"));
}

#[test]
fn test_dom_compound_event_name_lowercase() {
    // Compound event names like onMouseDown should become "mousedown" (all lowercase)
    let code = transform_dom(r#"<div onMouseDown={handler}>test</div>"#);
    assert!(
        code.contains("$$mousedown"),
        "onMouseDown should produce lowercase 'mousedown', got: {}",
        code
    );
}

// ============================================================================
// DOM: Dynamic Children
// ============================================================================

#[test]
fn test_dom_dynamic_text_child() {
    let code = transform_dom(r#"<div>{count()}</div>"#);
    assert!(code.contains("insert"));
    assert!(code.contains("count()"));
}

#[test]
fn test_dom_multiple_dynamic_children() {
    let code = transform_dom(r#"<div>{a()}{b()}</div>"#);
    assert!(code.contains("a()"));
    assert!(code.contains("b()"));
}

#[test]
fn test_dom_mixed_children() {
    let code = transform_dom(r#"<div>Hello {name()}!</div>"#);
    // Static text in template, dynamic inserted
    assert!(code.contains("insert"));
    assert!(code.contains("name()"));
}

#[test]
fn test_dom_mixed_text_inserts_before_marker() {
    let code = transform_dom(r#"<div>Hello {name()}!</div>"#);
    // Whitespace is preserved: "Hello " keeps trailing space
    assert!(
        code.contains("<div>Hello <!>!</div>"),
        "Template should preserve space before marker, got: {}",
        code
    );
    assert!(
        code.contains("insert(_el$1, () => name(), _el$2)"),
        "Should insert with marker, got: {}",
        code
    );
}

#[test]
fn test_dom_nested_element_after_text_walks_next_sibling() {
    let code = transform_dom(r#"<div>Hello <span class={style()}>world</span></div>"#);
    assert!(code.contains("firstChild.nextSibling"));
    assert!(code.contains("style()"));
}

#[test]
fn test_dom_component_between_elements_inserts_before_marker() {
    let code = transform_dom(r#"<div><span>text</span><Counter /><p>more</p></div>"#);
    assert!(code.contains("<span>text</span><!><p>more</p>"));
    assert!(code.contains("insert(_el$1, createComponent(Counter, {}), _el$2)"));
}

// ============================================================================
// DOM: Refs
// ============================================================================

#[test]
fn test_dom_ref_variable() {
    let code = transform_dom(r#"<div ref={myRef}>content</div>"#);
    assert!(code.contains("myRef"));
}

#[test]
fn test_dom_ref_callback() {
    let code = transform_dom(r#"<div ref={el => setRef(el)}>content</div>"#);
    assert!(code.contains("setRef"));
}

#[test]
fn test_dom_ref_const_identifier_no_assignment_fallback() {
    let code = transform_dom(
        r#"
        const [header, setHeader] = createSignal();
        <div ref={setHeader}>content</div>
        "#,
    );
    assert!(code.contains("setHeader"), "Output was:\n{code}");
    assert!(!code.contains("setHeader=_el$"), "Output was:\n{code}");
}

#[test]
fn test_component_ref_const_identifier_passed_directly() {
    // Component refs for const bindings (signal setters) should be passed directly
    // without the typeof ternary wrapper, matching babel-plugin-jsx-dom-expressions.
    let code = transform_dom(
        r#"
        const [ref, setRef] = createSignal();
        const Child = (p) => p;
        <Child ref={setRef}>content</Child>
        "#,
    );
    eprintln!("=== Component ref const output ===\n{code}\n===");
    assert!(code.contains("ref: setRef"), "Expected direct ref pass, output was:\n{code}");
    assert!(!code.contains("typeof"), "Should not have typeof check for const ref, output was:\n{code}");
}

#[test]
fn test_component_ref_signal_setter_realistic() {
    // Realistic pattern from agents-sidebar.tsx
    let code = transform_dom(
        r#"
        import { createSignal } from "solid-js";
        const [searchInputRef, setSearchInputRef] = createSignal(null);
        const Input = (p) => p;
        <Input ref={setSearchInputRef} placeholder="Search" />
        "#,
    );
    eprintln!("=== Realistic component ref output ===\n{code}\n===");
    assert!(!code.contains("typeof"), "Should not have typeof check for const signal setter ref, output was:\n{code}");
    assert!(!code.contains("setSearchInputRef ="), "Should not assign to const, output was:\n{code}");
}

#[test]
fn test_component_ref_let_variable_gets_ternary() {
    // Component refs for let variables should still get the typeof ternary.
    let code = transform_dom(
        r#"
        let childRef;
        const Child = (p) => p;
        <Child ref={childRef}>content</Child>
        "#,
    );
    assert!(code.contains("typeof"), "Expected typeof check for let ref, output was:\n{code}");
}

#[test]
fn test_component_ref_arrow_function_passed_directly() {
    // Arrow function refs should be passed directly.
    let code = transform_dom(
        r#"
        let el;
        const Child = (p) => p;
        <Child ref={e => el = e}>content</Child>
        "#,
    );
    assert!(!code.contains("typeof"), "Should not have typeof check for arrow function ref, output was:\n{code}");
}

#[test]
fn test_dom_does_not_duplicate_existing_solid_web_imports() {
    let code = transform_dom(
        r#"
        import { mergeProps } from "solid-js/web";
        const props = {};
        const Comp = (p) => p;
        <Comp {...props} a={1} />
        "#,
    );
    assert!(code.contains("solid-js/web"), "Output was:\n{code}");
    assert_eq!(code.matches("solid-js/web").count(), 1, "Output was:\n{code}");
}

#[test]
fn test_dom_does_not_duplicate_mergeprops_from_solid_js() {
    // mergeProps can be imported from "solid-js" (re-export) instead of "solid-js/web"
    // The transformer should not add a duplicate import
    let code = transform_dom(
        r#"
        import { mergeProps } from "solid-js";
        const props = {};
        const Comp = (p) => p;
        <Comp {...props} a={1} />
        "#,
    );
    // Should use the existing mergeProps import, not add a new one
    assert!(
        !code.contains("mergeProps } from \"solid-js/web\""),
        "Should not add duplicate mergeProps import from solid-js/web. Output was:\n{code}"
    );
    // The existing import should be preserved
    assert!(
        code.contains("mergeProps } from \"solid-js\""),
        "Should preserve the existing mergeProps import from solid-js. Output was:\n{code}"
    );
}

#[test]
fn test_dom_namespace_import_from_solid_web_adds_separate_helper_import() {
    let code = transform_dom(
        r#"
        import * as Solid from "solid-js/web";
        <div>{count()}</div>
        "#,
    );

    assert!(
        !code.contains("* as Solid, {") && !code.contains("* as Solid , {"),
        "Should not merge named helpers into namespace import. Output was:\n{code}"
    );
    assert_eq!(
        code.matches("solid-js/web").count(),
        2,
        "Expected namespace import + separate helper import. Output was:\n{code}"
    );
}

// ============================================================================
// DOM: Style
// ============================================================================

#[test]
fn test_dom_style_string() {
    let code = transform_dom(r#"<div style="color: red">content</div>"#);
    assert!(code.contains("style=\"color: red\""));
}

#[test]
fn test_dom_style_object_static() {
    let code = transform_dom(r#"<div style={{ color: 'red', fontSize: 14 }}>content</div>"#);
    // Should be inlined as static style
    assert!(code.contains("color: red"));
    assert!(code.contains("font-size: 14px"));
}

#[test]
fn test_dom_style_object_dynamic() {
    let code = transform_dom(r#"<div style={styles()}>content</div>"#);
    assert!(code.contains("style("));
    assert!(code.contains("styles()"));
}

// ============================================================================
// DOM: innerHTML/textContent
// ============================================================================

#[test]
fn test_dom_innerhtml() {
    let code = transform_dom(r#"<div innerHTML={html} />"#);
    assert!(code.contains(".innerHTML"));
    assert!(code.contains("html"));
}

#[test]
fn test_dom_textcontent() {
    let code = transform_dom(r#"<div textContent={text} />"#);
    assert!(code.contains(".textContent"));
    assert!(code.contains("text"));
}

// ============================================================================
// DOM: Spread
// ============================================================================

#[test]
fn test_dom_spread() {
    let code = transform_dom(r#"<div {...props}>content</div>"#);
    assert!(code.contains("spread"));
    assert!(code.contains("props"));
}

// ============================================================================
// DOM: Nested Dynamic Elements
// ============================================================================

#[test]
fn test_dom_nested_dynamic_element() {
    let code = transform_dom(r#"<div><span class={style()}>nested</span></div>"#);
    // Should walk to nested element
    assert!(code.contains("firstChild"));
    assert!(code.contains("style()"));
}

#[test]
fn test_dom_deeply_nested() {
    let code = transform_dom(r#"<div><span><a href={url()}>link</a></span></div>"#);
    // Should walk: firstChild.firstChild
    assert!(code.contains("firstChild"));
    assert!(code.contains("url()"));
}

// ============================================================================
// DOM: Components
// ============================================================================

#[test]
fn test_dom_component_basic() {
    let code = transform_dom(r#"<Button />"#);
    assert!(code.contains("createComponent"));
    assert!(code.contains("Button"));
}

#[test]
fn test_dom_component_with_props() {
    let code = transform_dom(r#"<Button onClick={handler} label="Click" />"#);
    assert!(code.contains("createComponent"));
    assert!(code.contains("onClick"));
    assert!(code.contains("handler"));
    assert!(code.contains("label"));
}

#[test]
fn test_dom_component_with_children() {
    let code = transform_dom(r#"<Button>Click me</Button>"#);
    assert!(code.contains("createComponent"));
    assert!(code.contains("children"));
    assert!(code.contains("Click me"));
}

#[test]
fn test_dom_component_with_jsx_children() {
    let code = transform_dom(r#"<Button><span>icon</span> Click</Button>"#);
    assert!(code.contains("createComponent"));
    // Children should include the span template
    assert!(code.contains("template"));
}

#[test]
fn test_dom_component_nested_in_element() {
    // This is the critical test - components inside native elements
    // should be transformed with insert() + createComponent()
    let code = transform_dom(r#"<main><Counter /></main>"#);

    // Should have a template for the parent element with a placeholder marker
    assert!(
        code.contains("template"),
        "Should create template for parent element"
    );
    assert!(
        code.contains("<main>"),
        "Template should contain main element"
    );

    // The component should be transformed with createComponent
    assert!(
        code.contains("createComponent"),
        "Should use createComponent for Counter"
    );
    assert!(
        code.contains("Counter"),
        "Should reference Counter component"
    );

    // Should use insert() to place the component in the DOM
    assert!(
        code.contains("insert("),
        "Should use insert() for dynamic component child"
    );

    // Should NOT have <Counter> as literal HTML in the template
    assert!(
        !code.contains("<Counter>"),
        "Counter should NOT be literal HTML in template"
    );
}

/// Babel-parity regression test.
///
/// When a host element's only child is a single component, Babel emits the
/// template *without* a placeholder comment and `insert(parent, value)` with
/// no marker — the component fills the whole parent, and SolidJS appends.
///
/// OXC was instead emitting `template(`<main><!></main>`)` and
/// `insert(parent, value, parent.firstChild)`. With a marker present, the
/// hydration walker in `insertExpression` walks `nextSibling` chains looking
/// for the marker. The SSR runtime never emits a corresponding comment in
/// the output HTML (the SSR template for `<main>{x}</main>` is plain
/// `<main>${x}</main>`), so on the client `_el$2 = _el$1.firstChild` ends up
/// being the first *real* SSR-rendered child of the host element. The walk
/// then falls off the end of the sibling chain and crashes with
/// `Cannot read properties of null (reading 'nextSibling')`.
///
/// Surfaced live by hydrating Gothab's navbar (`<nav><Show ...>...</Show></nav>`).
#[test]
fn test_dom_single_component_child_no_marker() {
    let code = transform_dom(r#"function App() { return <main><Counter /></main>; }"#);

    // Template must NOT contain a placeholder comment — Babel emits
    // `template(`<main>`)` for this case.
    assert!(
        !code.contains("<main><!>"),
        "Template should not contain a placeholder comment when the element's \
         sole child is a component:\n{code}"
    );

    // No marker variable should be created (no `_el$2 = _el$1.firstChild`).
    assert!(
        !code.contains(".firstChild"),
        "Should not capture a marker via .firstChild for a single-component child:\n{code}"
    );

    // `insert` must be called with exactly two arguments — parent and value.
    assert!(
        code.contains("insert(_el$1, createComponent(Counter, {}))"),
        "Expected `insert(_el$1, createComponent(Counter, {{}}))` (no marker arg):\n{code}"
    );
}

/// Same bug, expressed against `<nav>` to mirror the exact JSX shape that
/// blew up on the live page (`<nav><Show ...>…</Show></nav>`).
#[test]
fn test_dom_single_show_child_no_marker() {
    let code = transform_dom(
        r#"function App() { return <nav><Show when={x}>hi</Show></nav>; }"#,
    );

    assert!(
        !code.contains("<nav><!>"),
        "Template should not contain a placeholder comment when the element's \
         sole child is a <Show> component:\n{code}"
    );
    assert!(
        !code.contains(".firstChild"),
        "Should not capture a marker for a single-component child:\n{code}"
    );
    // The Show invocation lives inside an `insert(parent, value)` call with
    // no marker.
    assert!(
        code.contains("insert(_el$1, createComponent(Show,"),
        "Expected `insert(_el$1, createComponent(Show, ...))` (no marker arg):\n{code}"
    );
}

#[test]
fn test_dom_multiple_components_nested_in_element() {
    let code = transform_dom(r#"<div><Header /><Content /><Footer /></div>"#);

    // Should create template with placeholder
    assert!(code.contains("template"));
    assert!(code.contains("<div>"));

    // All components should be transformed
    assert!(code.contains("createComponent"));

    // Should have multiple insert calls
    let insert_count = code.matches("insert(").count();
    assert!(
        insert_count >= 3,
        "Should have insert() for each component, found {}",
        insert_count
    );

    // Components should NOT be literal HTML
    assert!(!code.contains("<Header>"));
    assert!(!code.contains("<Content>"));
    assert!(!code.contains("<Footer>"));
}

#[test]
fn test_dom_mixed_elements_and_components() {
    let code = transform_dom(r#"<div><span>text</span><Counter /><p>more</p></div>"#);

    // Native elements should be in template
    assert!(code.contains("<span>text</span>"));
    assert!(code.contains("<p>more</p>"));

    // Component should use createComponent + insert
    assert!(code.contains("createComponent"));
    assert!(code.contains("Counter"));
    assert!(code.contains("insert("));

    // Counter should NOT be literal HTML
    assert!(!code.contains("<Counter>"));
}

#[test]
fn test_dom_deeply_nested_component() {
    // Component nested multiple levels deep. The inner element has a single
    // dynamic child (the component), so it should NOT have a placeholder
    // comment in the template — Babel emits `<div><main></main></div>` for
    // this and inserts the component into <main> with no marker arg.
    let code = transform_dom(r#"<div><main><Counter /></main></div>"#);

    assert!(
        code.contains("<div><main></main></div>"),
        "Template should match Babel: bare <main> with no <!> placeholder \
         when its sole child is a component:\n{code}"
    );

    // Should walk to the parent element (main)
    assert!(
        code.contains("firstChild"),
        "Should walk to nested parent element"
    );

    // Should insert the component
    assert!(code.contains("createComponent"));
    assert!(code.contains("Counter"));
    assert!(code.contains("insert("));

    // Counter should NOT be literal HTML
    assert!(!code.contains("<Counter>"));
}

#[test]
fn test_dom_very_deeply_nested_component() {
    let code = transform_dom(r#"<div><section><article><MyComponent /></article></section></div>"#);

    // Same single-dynamic-child rule applies to the deepest <article>:
    // no `<!>` placeholder, just <article></article>.
    assert!(
        code.contains("<div><section><article></article></section></div>"),
        "Template should match Babel: no <!> placeholder for the article \
         when its sole child is a component:\n{code}"
    );

    // Should walk through nested elements
    assert!(
        code.contains("firstChild.firstChild"),
        "Should walk through multiple levels"
    );

    // Should use createComponent + insert
    assert!(code.contains("createComponent"));
    assert!(code.contains("MyComponent"));
    assert!(code.contains("insert("));

    // Component should NOT be literal HTML
    assert!(!code.contains("<MyComponent>"));
}

// ============================================================================
// DOM: Built-in Components
// ============================================================================

#[test]
fn test_dom_for() {
    let code = transform_dom(r#"<For each={items}>{item => <div>{item}</div>}</For>"#);
    assert!(code.contains("createComponent"));
    assert!(code.contains("For"));
    assert!(code.contains("get each()"));
    assert!(code.contains("items"));
}

#[test]
fn test_dom_show() {
    let code = transform_dom(r#"<Show when={visible}><div>shown</div></Show>"#);
    assert!(code.contains("createComponent"));
    assert!(code.contains("Show"));
    assert!(code.contains("get when()"));
    assert!(code.contains("visible"));
    assert!(
        code.contains("cloneNode(true)"),
        "Show children should be transformed JSX output"
    );
}

#[test]
fn test_dom_show_with_fallback() {
    let code = transform_dom(
        r#"<Show when={visible} fallback={<div>hidden</div>}><div>shown</div></Show>"#,
    );
    assert!(code.contains("Show"));
    assert!(code.contains("get fallback()"));
    assert!(
        code.contains("cloneNode(true)"),
        "Show fallback/children JSX should be transformed"
    );
}

#[test]
fn test_dom_show_with_event_child() {
    let code =
        transform_dom(r#"<Show when={visible}><button onClick={handler}>ok</button></Show>"#);
    assert!(code.contains("Show"));
    assert!(
        code.contains("$$click"),
        "Event handler should compile to delegated $$click assignment"
    );
    assert!(
        code.contains("return _el$"),
        "Show child should return the created element (not just a side effect)"
    );
}

#[test]
fn test_dom_switch_match() {
    let code =
        transform_dom(r#"<Switch><Match when={a}>A</Match><Match when={b}>B</Match></Switch>"#);
    assert!(code.contains("Switch"));
    assert!(code.contains("Match"));
}

#[test]
fn test_dom_index() {
    let code = transform_dom(r#"<Index each={items}>{(item, i) => <div>{i()}</div>}</Index>"#);
    assert!(code.contains("Index"));
    assert!(code.contains("get each()"));
}

#[test]
fn test_dom_suspense() {
    let code =
        transform_dom(r#"<Suspense fallback={<div>Loading...</div>}><Content /></Suspense>"#);
    assert!(code.contains("Suspense"));
    assert!(code.contains("get fallback()"));
}

#[test]
fn test_dom_error_boundary() {
    let code = transform_dom(
        r#"<ErrorBoundary fallback={err => <div>{err}</div>}><Content /></ErrorBoundary>"#,
    );
    assert!(code.contains("ErrorBoundary"));
}

// ============================================================================
// SSR: Basic Elements
// ============================================================================

#[test]
fn test_ssr_static_element() {
    let code = transform_ssr(r#"<div class="hello">world</div>"#);
    // SSR should output string or ssr template
    assert!(code.contains("<div") && code.contains("</div>"));
}

#[test]
fn test_ssr_dynamic_attribute() {
    let code = transform_ssr(r#"<div class={style()}>content</div>"#);
    assert!(code.contains("ssr`"));
    assert!(code.contains("escape"));
    assert!(code.contains("style()"));
}

#[test]
fn test_ssr_dynamic_child() {
    let code = transform_ssr(r#"<div>{count()}</div>"#);
    assert!(code.contains("ssr`"));
    assert!(code.contains("escape"));
    assert!(code.contains("count()"));
}

#[test]
fn test_ssr_component() {
    let code = transform_ssr(r#"<Button onClick={handler}>Click</Button>"#);
    assert!(code.contains("createComponent"));
    assert!(code.contains("Button"));
}

#[test]
fn test_ssr_for() {
    let code = transform_ssr(r#"<For each={items}>{item => <li>{item}</li>}</For>"#);
    assert!(code.contains("For"));
    assert!(code.contains("get each()"));
}

// ============================================================================
// Edge Cases
// ============================================================================

#[test]
fn test_empty_fragment() {
    let code = transform_dom(r#"<></>"#);
    // Empty fragment should produce minimal output
    assert!(!code.is_empty());
}

#[test]
fn test_fragment_with_children() {
    let code = transform_dom(r#"<><div>a</div><div>b</div></>"#);
    assert!(code.contains("template"));
}

#[test]
fn test_fragment_multiple_root_elements_declare_el_bindings() {
    // Regression: multi-root fragments must not merge into a single template output
    // (template() only returns the first root), and must not reference undeclared _el$ bindings.
    let code = transform_dom(r#"<><div class={a()}></div><div class={b()}></div></>"#);

    assert!(code.contains("a()"), "Output was:\n{code}");
    assert!(code.contains("b()"), "Output was:\n{code}");

    // Collect all _el$<n> references and ensure each has a corresponding `const _el$<n>`.
    let mut referenced: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let bytes = code.as_bytes();
    let mut i = 0usize;
    while i + 4 < bytes.len() {
        if bytes[i] == b'_' && bytes[i + 1] == b'e' && bytes[i + 2] == b'l' && bytes[i + 3] == b'$'
        {
            let mut j = i + 4;
            if j < bytes.len() && bytes[j].is_ascii_digit() {
                while j < bytes.len() && bytes[j].is_ascii_digit() {
                    j += 1;
                }
                referenced.insert(code[i..j].to_string());
                i = j;
                continue;
            }
        }
        i += 1;
    }

    for id in referenced {
        assert!(
            code.contains(&format!("const {id}")),
            "Expected declaration for {id} in output:\n{code}"
        );
    }
}

#[test]
fn test_svg_element() {
    let code = transform_dom(r#"<svg><circle cx="50" cy="50" r="40" /></svg>"#);
    assert!(code.contains("svg"));
    assert!(code.contains("circle"));
}

#[test]
fn test_custom_element() {
    let code = transform_dom(r#"<my-element attr="value">content</my-element>"#);
    assert!(code.contains("my-element"));
}

#[test]
fn test_namespaced_attribute() {
    let code = transform_dom(
        r##"<svg xmlns:xlink="http://www.w3.org/1999/xlink"><use xlink:href="#id" /></svg>"##,
    );
    assert!(code.contains("xlink:href"));
}

#[test]
fn test_whitespace_handling() {
    let code = transform_dom(
        r#"<div>
        hello
        world
    </div>"#,
    );
    // Should handle whitespace appropriately
    assert!(code.contains("hello"));
}

#[test]
fn test_special_characters() {
    let code = transform_dom(r#"<div>&amp; &lt; &gt;</div>"#);
    // HTML entities should be preserved or properly escaped
    assert!(!code.is_empty());
}

// ============================================================================
// Import Generation
// ============================================================================

#[test]
fn test_dom_imports_template() {
    let code = transform_dom(r#"<div>hello</div>"#);
    assert!(code.contains("import"));
    assert!(code.contains("template"));
    assert!(code.contains("solid-js/web"));
}

#[test]
fn test_dom_imports_insert() {
    let code = transform_dom(r#"<div>{dynamic()}</div>"#);
    assert!(code.contains("insert"));
}

#[test]
fn test_dom_imports_effect() {
    let code = transform_dom(r#"<div class={dynamic()}>content</div>"#);
    assert!(code.contains("effect"));
}

#[test]
fn test_dom_imports_delegate_events() {
    let code = transform_dom(r#"<button onClick={handler}>click</button>"#);
    assert!(code.contains("delegateEvents"));
}

#[test]
fn test_ssr_imports() {
    let code = transform_ssr(r#"<div>{count()}</div>"#);
    assert!(code.contains("import"));
    assert!(code.contains("ssr"));
    assert!(code.contains("escape"));
}

#[test]
fn test_ssr_namespace_import_from_solid_web_adds_separate_helper_import() {
    let code = transform_ssr(
        r#"
        import * as Solid from "solid-js/web";
        <div>{count()}</div>
        "#,
    );

    assert!(
        !code.contains("* as Solid, {") && !code.contains("* as Solid , {"),
        "Should not merge named helpers into namespace import. Output was:\n{code}"
    );
    assert_eq!(
        code.matches("solid-js/web").count(),
        2,
        "Expected namespace import + separate helper import. Output was:\n{code}"
    );
}

#[test]
fn test_ssr_namespace_import_with_member_usage_keeps_separate_helper_import() {
    let code = transform_ssr(
        r#"
        import * as Solid from "solid-js/web";
        const docType = Solid.ssr("<!DOCTYPE html>");
        const stream = () => <>{docType}{children()}</>;
        "#,
    );

    assert!(
        !code.contains("* as Solid, {") && !code.contains("* as Solid , {"),
        "Should not merge named helpers into namespace import. Output was:\n{code}"
    );
    assert_eq!(
        code.matches("solid-js/web").count(),
        2,
        "Expected namespace import + separate helper import. Output was:\n{code}"
    );
}

#[test]
fn test_dom_source_map_generation() {
    let options = TransformOptions {
        filename: "input.jsx",
        source_map: true,
        ..TransformOptions::solid_defaults()
    };
    let result = transform(r#"<div>{x()}</div>"#, Some(options));
    assert!(result.map.is_some(), "expected source map to be generated");
}

#[test]
fn test_ssr_source_map_generation() {
    let options = TransformOptions {
        generate: GenerateMode::Ssr,
        filename: "input.jsx",
        source_map: true,
        ..TransformOptions::solid_defaults()
    };
    let result = transform(r#"<div>{x()}</div>"#, Some(options));
    assert!(result.map.is_some(), "expected source map to be generated");
}

// ============================================================================
// Regression Tests for Nested Dynamic Content
// ============================================================================

#[test]
fn test_dom_nested_dynamic_content() {
    // {x()} inside nested <span> should produce insert() without marker (single dynamic child)
    let code = transform_dom(r#"<div><span>{x()}</span></div>"#);

    // Template should have span without marker (single dynamic child optimization)
    assert!(
        code.contains("<span></span>"),
        "Template should have empty span (no marker for single dynamic child), got: {}",
        code
    );

    // Should walk to span element
    assert!(
        code.contains("firstChild"),
        "Should walk to span with firstChild, got: {}",
        code
    );

    // Should insert into span without marker argument
    assert!(
        code.contains("insert("),
        "Should have insert() call, got: {}",
        code
    );
    assert!(code.contains("x()"), "Should reference x(), got: {}", code);
}

#[test]
fn test_dom_two_siblings_with_events() {
    // Bug: second button should use firstChild.nextSibling not root.nextSibling
    let code = transform_dom(
        r#"<div><button onClick={() => 1}>A</button><button onClick={() => 2}>B</button></div>"#,
    );

    // Should have proper sibling traversal
    assert!(
        code.contains("firstChild"),
        "Should walk to first button, got: {}",
        code
    );
    // Second button should chain from first: firstChild.nextSibling
    assert!(
        code.contains("firstChild.nextSibling"),
        "Should walk to second button via firstChild.nextSibling, got: {}",
        code
    );
}

// ============================================================================
// Regression: components nested inside HTML elements via a built-in component
// (Show, etc.) used to lose all props and children — `transform_element`'s
// nested-component fallback emitted `createComponent(Tag, {})`. Discovered
// while integrating into Gothab. Fixed by making the child-transformer in
// element.rs recurse through `transform_jsx_child`.
// ============================================================================

fn transform_ssr_hydratable_re(source: &str) -> String {
    let options = TransformOptions {
        generate: GenerateMode::Ssr,
        hydratable: true,
        ..TransformOptions::solid_defaults()
    };
    let result = transform(source, Some(options));
    result.code
}

#[test]
fn ssr_regression_nested_component_inside_html_element() {
    // Minimal repro: <html-element><Component><Component .../></Component></html-element>
    let code = transform_ssr_hydratable_re(
        r#"<nav class={s.navbar}><Show when={c}><A href="/x" class={s.a}>hi</A></Show></nav>"#,
    );
    assert!(
        code.contains(r#"href: "/x""#),
        "Inner component lost its props. Output:\n{}",
        code
    );
    assert!(
        code.contains(r#"children: "hi""#) || code.contains("get children()"),
        "Inner component lost its children. Output:\n{}",
        code
    );
    assert!(
        !code.contains("createComponent(A, {})"),
        "Inner component A emitted with empty props/children. Output:\n{}",
        code
    );
}

#[test]
fn ssr_regression_nested_component_with_jsx_children() {
    // <html-element><Component><Component>...<Component/></Component></Component></html-element>
    let code = transform_ssr_hydratable_re(
        r#"<nav><Show when={c}><A href="/x" class={s.a}><img src="/y"/><Word class={s.w}/></A></Show></nav>"#,
    );
    assert!(
        code.contains(r#"href: "/x""#),
        "Inner component lost its props. Output:\n{}",
        code
    );
    assert!(
        code.contains("get children()"),
        "Inner component lost its children getter. Output:\n{}",
        code
    );
    // Grandchild component must also keep its props
    assert!(
        code.contains("get class()") && code.contains("return s.w"),
        "Grandchild component (Word) lost its class prop. Output:\n{}",
        code
    );
}

#[test]
fn ssr_regression_element_with_spread_uses_merge_props() {
    // <a {...rest} href={x} link aria-current={y}/> — body of @solidjs/router's
    // <A>. Must wrap with mergeProps so getters stay lazy and rest.children
    // flows through to the runtime.
    let code = transform_ssr_hydratable_re(
        r#"const x = <a {...rest} href={href() || props.href} link classList={cl}/>;"#,
    );
    assert!(
        code.contains("mergeProps(rest"),
        "Expected mergeProps wrapping spread. Got:\n{}",
        code
    );
    assert!(
        code.contains("get href()"),
        "Expected dynamic href as getter. Got:\n{}",
        code
    );
    assert!(
        code.contains("link: true"),
        "Expected boolean link prop. Got:\n{}",
        code
    );
    // Self-closing with spread → undefined children, not null.
    assert!(
        code.contains(", undefined,"),
        "Expected `undefined` (not `null`) as children arg so runtime reads props.children. Got:\n{}",
        code
    );
    assert!(
        !code.contains("...rest"),
        "Should NOT eagerly spread `rest`; that loses reactivity. Got:\n{}",
        code
    );
}

#[test]
fn ssr_regression_solitary_dynamic_child_has_no_markers() {
    // A parent with a SINGLE meaningful child must NOT wrap that child with
    // `<!--$-->`/`<!--/-->`. Babel's rule: `markers = hydratable && multi`,
    // and `multi` only when there are >1 meaningful children. When OXC
    // mismatched this rule and emitted markers around solitary children,
    // `getNextMarker` couldn't locate them and the runtime fell back to a
    // full client-side re-render (visible "HTML flashing" on first paint).
    let code = transform_ssr_hydratable_re(r#"const x = <div>{value}</div>;"#);
    assert!(
        !code.contains("<!--$-->"),
        "Solitary dynamic child must not be wrapped with hydration markers. Got:\n{}",
        code
    );
    assert!(
        !code.contains("<!--/-->"),
        "Solitary dynamic child must not be wrapped with hydration markers. Got:\n{}",
        code
    );

    // Component as a sole child — same rule.
    let code = transform_ssr_hydratable_re(r#"const x = <div><Show when={c}>hi</Show></div>;"#);
    assert!(
        !code.contains("<!--$-->"),
        "Solitary component child must not be wrapped with hydration markers. Got:\n{}",
        code
    );
}

#[test]
fn ssr_regression_multi_dynamic_children_get_markers() {
    // With >1 meaningful children, dynamic children MUST be wrapped with
    // `<!--$-->`/`<!--/-->` so `getNextMarker` can locate them at hydration
    // time. Static text and native elements stay unwrapped.
    let code = transform_ssr_hydratable_re(
        r#"const x = <div>before{value}after</div>;"#,
    );
    assert!(
        code.contains("<!--$-->") && code.contains("<!--/-->"),
        "Multi-children: dynamic child must be wrapped. Got:\n{}",
        code
    );
}

#[test]
fn ssr_regression_markers_use_dollar_sign() {
    // The runtime `getNextMarker` matches `v === "$"` and `v === "/"`; using
    // any other token (e.g. `<!--#-->`) leaves the markers unrecognised and
    // forces a client-side re-render.
    let code = transform_ssr_hydratable_re(
        r#"const x = <div>before{value}after</div>;"#,
    );
    assert!(
        !code.contains("<!--#-->"),
        "Wrong marker token; runtime expects `<!--$-->`. Got:\n{}",
        code
    );
}

#[test]
fn ssr_regression_no_markers_inside_open_tag() {
    // Hydration markers must never appear inside an opening tag; the browser
    // would silently mis-parse the result and hydration would mismatch.
    // `ssrHydrationKey()`, `ssrStyle()`, `ssrClassList()`, and `ssrAttribute()`
    // all interpolate inside the tag and must NOT be marker-wrapped.
    let code = transform_ssr_hydratable_re(
        r#"const x = <div style={s} classList={cl} disabled={d}>{value}</div>;"#,
    );
    // The tag-internal sequences are emitted before `>` closes the open tag.
    // Crude check: no `<!--$-->` should immediately follow `<div`.
    assert!(
        !code.contains("<div<!--$-->"),
        "Hydration marker leaked inside opening tag. Got:\n{}",
        code
    );
    assert!(
        !code.contains("<!--$-->ssrHydrationKey")
            && !code.contains("<!--$-->ssrStyle")
            && !code.contains("<!--$-->ssrClassList")
            && !code.contains("<!--$-->ssrAttribute"),
        "In-tag helper interpolations must not be wrapped with hydration markers. Got:\n{}",
        code
    );
}

fn transform_dom_hydratable_re(source: &str) -> String {
    let options = TransformOptions {
        generate: GenerateMode::Dom,
        hydratable: true,
        ..TransformOptions::solid_defaults()
    };
    let result = transform(source, Some(options));
    result.code
}

#[test]
fn dom_regression_hydratable_uses_get_next_element() {
    // In hydratable DOM mode the compiled output must walk the SSR-produced
    // DOM via `getNextElement(_tmpl$N)` instead of `_tmpl$N.cloneNode(true)`.
    // `cloneNode` always builds fresh DOM, which the dev runtime traps with
    // "Failed attempt to create new DOM elements during hydration" — and in
    // production silently re-renders the whole page client-side, defeating
    // SSR entirely.
    let code = transform_dom_hydratable_re(
        r#"const x = <div class="hello">world</div>;"#,
    );
    assert!(
        code.contains("getNextElement(_tmpl$1)"),
        "Hydratable DOM must call getNextElement, not cloneNode. Got:\n{}",
        code
    );
    assert!(
        !code.contains("cloneNode"),
        "Hydratable DOM must NOT call cloneNode (would build fresh DOM and break hydration). Got:\n{}",
        code
    );
    assert!(
        code.contains("getNextElement"),
        "Expected `getNextElement` to be imported. Got:\n{}",
        code
    );
}

#[test]
fn dom_regression_non_hydratable_still_uses_clone_node() {
    // Default (non-hydratable) DOM emits cloneNode — preserves the optimized
    // path for client-only apps that don't SSR.
    let code = transform_dom(r#"<div class="hello">world</div>"#);
    assert!(
        code.contains("cloneNode(true)"),
        "Non-hydratable DOM must call cloneNode. Got:\n{}",
        code
    );
    assert!(
        !code.contains("getNextElement"),
        "Non-hydratable DOM must NOT pull in getNextElement. Got:\n{}",
        code
    );
}

#[test]
fn dom_regression_spread_with_attrs_uses_merge_props() {
    // When an element has any spread, every spreadable attribute must fold
    // into a single `spread(elem, mergeProps(rest, {...}), isSVG, hasChildren)`
    // call instead of being baked into the template or emitted as separate
    // setAttribute effects. Anything else loses reactivity (eager-evaluated
    // getters on `rest`) and breaks hydration key alignment with the SSR
    // output (the template ends up with extra static attrs that the SSR HTML
    // doesn't have).
    //
    // Source modeled after `@solidjs/router`'s <A>:
    //   <a {...rest} href={x()} link aria-current={ac}/>
    let code = transform_dom_hydratable_re(
        r#"function A(props) {
            const rest = props.rest;
            const x = () => props.href;
            const ac = props.current;
            return <a {...rest} href={x()} link aria-current={ac}/>;
        }"#,
    );

    // Single spread() call rather than spread(...) followed by effect()s.
    let spread_calls = code.matches("spread(").count();
    assert!(
        spread_calls >= 1,
        "Expected at least one spread() call. Got:\n{}",
        code
    );
    assert!(
        code.contains("mergeProps("),
        "Spread + extra attrs must fold into mergeProps(...). Got:\n{}",
        code
    );

    // Dynamic attr (function call) -> getter; static boolean attr -> "" string.
    assert!(
        code.contains("get href()") || code.contains("get \"href\"()"),
        "Dynamic attr `href={{x()}}` must become a getter inside mergeProps. Got:\n{}",
        code
    );
    assert!(
        code.contains("link: \"\"") || code.contains("\"link\": \"\""),
        "Boolean attr `link` (no value) must become an empty-string property. Got:\n{}",
        code
    );

    // Template must NOT bake the boolean attr in — the whole point is to let
    // the runtime spread set every attribute identically to how the SSR HTML
    // already has them, so hydration keys align.
    assert!(
        !code.contains("<a link>") && !code.contains("<a link=\"\">"),
        "Template must not pre-bake `link` when the element has a spread. Got:\n{}",
        code
    );

    // No separate setAttribute effects for spreadable attrs.
    assert!(
        !code.contains("setAttribute(\"href\""),
        "Dynamic attr should fold into mergeProps, not emit a separate setAttribute effect. Got:\n{}",
        code
    );

    // Hydratable mode must call runHydrationEvents() after the spread to
    // flush deferred event handler attachments.
    assert!(
        code.contains("runHydrationEvents()"),
        "Hydratable spread must emit runHydrationEvents(). Got:\n{}",
        code
    );
}

#[test]
fn ssr_regression_spread_element_is_emitted_bare() {
    // Babel's `createTemplate` short-circuits with `if (!result.template)
    // return result.exprs[0]`: when an SSR element has no static template
    // content (only a single `ssrElement(...)` call from a spread), it returns
    // the bare call expression rather than wrapping it in `ssr`${...}``.
    //
    // Wrapping produces the same final HTML but a different *expression
    // shape* — surrounding code (e.g. parents inserting this into a children
    // array, or hydration-aware traversal) can dispatch differently on
    // `CallExpression(ssrElement)` vs `TaggedTemplate(ssr, …)`. In a hydratable
    // build that drift can desync the SSR-side `getNextContextId()` allocation
    // from the DOM-side one and surface as
    //     `Hydration Mismatch. Unable to find DOM nodes for hydration key`
    // even though the rendered HTML is structurally identical.
    let code = transform_ssr_re(r#"function A(props) { return <a {...props.rest}/>; }"#);

    assert!(
        code.contains("ssrElement(\"a\""),
        "SSR spread element should still emit ssrElement(...). Got:\n{}",
        code
    );
    assert!(
        !code.contains("ssr`"),
        "Spread-only SSR result must not be wrapped in ssr`...`. Got:\n{}",
        code
    );
    assert!(
        !code.contains(", ssr }") && !code.contains("ssr,"),
        "Unused `ssr` helper should not be imported when the wrapping is skipped. Got:\n{}",
        code
    );
}

#[test]
fn ssr_regression_spread_child_is_not_wrapped_in_hydration_markers() {
    // Symptom Bart hit in Gothab: a `@gothab/ui` Checkbox shaped like
    //
    //   <label><input type="checkbox" {...rest}/>{children}</label>
    //
    // had its `onChange` silently swallowed after SSR + hydration. The
    // `change` event fired natively but the consumer's listener never ran.
    //
    // Cause: this transform was wrapping the spread `<input>` child in
    // `<!--$-->`/`<!--/-->` hydration markers in the parent's SSR template:
    //
    //   ssr`<label>` + `<!--$-->${ssrElement("input", …)}<!--/-->` + …
    //
    // The client-side OXC transform, however, treats a child native
    // element as a static template chunk — `<label><input ...>...` — so
    // after hydration `label.firstChild` resolves to the `<!--$-->`
    // comment instead of the `<input>`. Refs and event listeners get
    // wired to a comment node and silently no-op.
    //
    // Babel's reference plugin (`src/ssr/element.js:509-515`) explicitly
    // skips marker emission for children with `spreadElement: true`. OXC's
    // `process_jsx_children` was treating those exactly like component
    // children — both got markers — instead of the Babel split:
    // components yes, spread elements no.
    //
    // Pin the Babel-parity output so this can't regress.
    let code = transform_ssr_hydratable_re(
        r#"function C(props) { return <label><input type="checkbox" {...props.rest}/>{props.children}</label>; }"#,
    );

    // The `ssrElement("input", …)` call must appear directly in the
    // parent template without surrounding `<!--$-->`/`<!--/-->`.
    assert!(
        !code.contains("<!--$-->${ssrElement(\"input\""),
        "Spread element child must not be preceded by `<!--$-->` in the parent template. Got:\n{}",
        code,
    );
    assert!(
        !code.contains(", true)}<!--/-->"),
        "Spread element child must not be followed by `<!--/-->` in the parent template. Got:\n{}",
        code,
    );

    // Sibling text expression children are still wrapped — only the
    // spread element is exempted.
    assert!(
        code.contains("<!--$-->${escape(props.children)}<!--/-->"),
        "Sibling text-expression child should still get markers (multi-children + hydratable). Got:\n{}",
        code,
    );

    // Sanity: ssrElement is still emitted for the input itself.
    assert!(
        code.contains("ssrElement(\"input\""),
        "Spread element should still produce ssrElement(...). Got:\n{}",
        code,
    );
}

fn transform_ssr_re(source: &str) -> String {
    let options = TransformOptions {
        generate: GenerateMode::Ssr,
        hydratable: true,
        ..TransformOptions::solid_defaults()
    };
    transform(source, Some(options)).code
}

#[test]
fn ssr_regression_only_root_element_emits_hydration_key() {
    // **Hydration counter parity bug.** OXC SSR previously emitted
    // `ssrHydrationKey()` for *every* native element, including ones nested
    // inside another native element's template chunk. Babel only emits one
    // for the root of each `_$ssr(_tmpl$, …)` call, because the DOM-side
    // walks nested static elements via `firstChild`/`nextSibling` from the
    // parent's `getNextElement(_tmpl$)` — without a separate `getHydrationKey`
    // advance per element.
    //
    // When SSR over-emits, its `getNextContextId()` counter advances ahead of
    // the DOM's, so by the time both sides reach a later element with a
    // hydration key, the SSR-emitted `data-hk` and the DOM-requested key
    // disagree by the drift. Surface symptom:
    //   `Hydration Mismatch. Unable to find DOM nodes for hydration key …`
    // for an `<a></a>` (or whatever element first lies past the drift point),
    // even though the rendered HTML is structurally identical to Babel's.
    //
    // Each native nesting level used to add one extra advance; here we
    // expect a single `ssrHydrationKey()` for `<div><div><span/></div></div>`.
    let code = transform_ssr_hydratable_re(
        r#"const x = <div class="outer"><div class="inner"><span class="leaf"/></div></div>;"#,
    );
    let count = code.matches("ssrHydrationKey()").count();
    assert_eq!(
        count, 1,
        "Nested static elements must share the parent's hydration key; \
         expected exactly 1 ssrHydrationKey() call, got {}. Output:\n{}",
        count, code
    );
}

#[test]
fn dom_regression_spread_identifier_arg_is_not_arrow_wrapped() {
    // Babel's `processSpreads` uses `isDynamic({ checkMember: true,
    // checkCallExpressions: true })` to decide whether to wrap a spread
    // argument in an arrow function. A plain identifier (e.g. `rest` from
    // `splitProps`) is NOT considered dynamic by that rule, so Babel emits
    //     mergeProps(rest, {...})
    // not
    //     mergeProps(() => rest, {...}).
    //
    // This matters for hydration: `mergeProps` reads the first argument's
    // getters lazily on every property access. Wrapping `rest` in an arrow
    // changes when those reads happen relative to the rest of the setup
    // expressions, which in a hydratable build shifts
    // `sharedConfig.getNextContextId()` allocation order between SSR and DOM
    // and surfaces as
    //   `Hydration Mismatch. Unable to find DOM nodes for hydration key…`
    let code = transform_dom_hydratable_re(
        r#"function A(rest) { return <a {...rest} href={url}/>; }"#,
    );
    assert!(
        code.contains("mergeProps(rest,") || code.contains("mergeProps(rest)"),
        "Plain identifier spread arg must be passed bare to mergeProps. Got:\n{}",
        code
    );
    assert!(
        !code.contains("() => rest"),
        "Plain identifier spread arg must NOT be wrapped in an arrow. Got:\n{}",
        code
    );
    // Plain identifier attr value should also be a regular prop, not a getter.
    assert!(
        code.contains("href: url") || code.contains("\"href\": url"),
        "Plain identifier attr value should be a plain prop, not a getter. Got:\n{}",
        code
    );
    assert!(
        !code.contains("get href()"),
        "Plain identifier attr value should NOT become a getter. Got:\n{}",
        code
    );
}

#[test]
fn dom_regression_no_spread_no_run_hydration_events() {
    // Without a spread we know at compile time whether the element has any
    // event handler. A plain hydratable element with no events should NOT
    // emit runHydrationEvents() — that's reserved for the spread case.
    let code = transform_dom_hydratable_re(r#"<div class="hello">world</div>"#);
    assert!(
        !code.contains("runHydrationEvents"),
        "No-spread hydratable element should not pull in runHydrationEvents. Got:\n{}",
        code
    );
}

#[test]
fn dom_regression_hydratable_marker_pair_for_dynamic_among_static() {
    // When a dynamic insertion sits among static template content in
    // hydratable DOM mode, the client template must use a `<!$><!/>`
    // comment pair (not a single `<!>`) so the placeholders match the
    // SSR-emitted `<!--$-->...<!--/-->` markers. The runtime walks the
    // SSR DOM via `getNextMarker` which tracks comment depth between
    // those two markers.
    //
    // Source: `<div>before {value} after</div>` — the `{value}` insert
    // is wrapped by static text on both sides, forcing the marker path.
    let code = transform_dom_hydratable_re(r#"const x = <div>before {value} after</div>;"#);
    assert!(
        code.contains("<!$><!/>"),
        "Hydratable DOM must emit `<!$><!/>` marker pair for dynamic insertions among static content. Got:\n{}",
        code
    );
    assert!(
        !code.contains("<!>"),
        "Hydratable DOM must NOT emit single `<!>` placeholder (mismatches SSR's two-comment marker pair). Got:\n{}",
        code
    );
    assert!(
        code.contains("getNextMarker"),
        "Hydratable DOM must import and call `getNextMarker` to walk between the marker pair. Got:\n{}",
        code
    );
    // The destructured pair `[markerId, contentId] = getNextMarker(...)`
    // should appear, and the 4-arg insert should pass both names.
    assert!(
        code.contains("] = getNextMarker("),
        "Expected `[marker, content] = getNextMarker(...)` destructure. Got:\n{}",
        code
    );
}

#[test]
fn dom_regression_non_hydratable_uses_single_placeholder() {
    // The non-hydratable path keeps the optimized single-`<!>` placeholder
    // and the 3-arg insert form. We must not regress this — adding the
    // marker pair to client-only apps would inflate every template.
    let code = transform_dom(r#"const x = <div>before {value} after</div>;"#);
    assert!(
        code.contains("<!>"),
        "Non-hydratable DOM must emit single `<!>` placeholder. Got:\n{}",
        code
    );
    assert!(
        !code.contains("<!$>"),
        "Non-hydratable DOM must NOT emit `<!$>` (hydration-only marker). Got:\n{}",
        code
    );
    assert!(
        !code.contains("getNextMarker"),
        "Non-hydratable DOM must NOT pull in getNextMarker. Got:\n{}",
        code
    );
}

#[test]
fn dom_regression_hydratable_marker_pair_for_component_among_static() {
    // Component children are dynamic insertions. When mixed with static
    // siblings in hydratable mode, the same marker-pair rule applies as
    // for `{expressions}` — the template must contain `<!$><!/>` so the
    // SSR comment markers line up.
    let code = transform_dom_hydratable_re(
        r#"const x = <div>before <Show when={c}>hi</Show> after</div>;"#,
    );
    assert!(
        code.contains("<!$><!/>"),
        "Hydratable DOM must emit `<!$><!/>` for component child among static content. Got:\n{}",
        code
    );
    assert!(
        code.contains("getNextMarker"),
        "Hydratable DOM must call getNextMarker for component-among-static. Got:\n{}",
        code
    );
}

#[test]
fn dom_regression_hydratable_walker_reanchors_after_marker_pair() {
    // After a `<!$><!/>` marker pair, the walker for the next positional
    // access MUST chain off the close marker, not from the parent's
    // firstChild. The SSR DOM has variable-length content between each
    // pair, so a count-from-firstChild walk lands on the wrong target.
    //
    // Source: `<div>{a} <span/></div>` — the dynamic insert at position
    // 0–1 (the marker pair) is followed by a static `<span>` at
    // position 2. The span's accessor must be `<close-marker>.nextSibling`,
    // not `_el$.firstChild.nextSibling.nextSibling` (which in the SSR
    // DOM would step into the `<!--/-->` marker, not past it).
    let code = transform_dom_hydratable_re(r#"const x = <div>{a} <span/></div>;"#);

    // Some declaration must use the form `<close-marker-id>.nextSibling`.
    // We can't predict the exact uid number, but the pattern
    // `getNextMarker(...)` followed later by a `.nextSibling` chained off
    // one of the destructured marker names is the signature of the fix.
    assert!(
        code.contains("getNextMarker"),
        "Expected `getNextMarker` import for marker-pair walking. Got:\n{}",
        code
    );

    // Critically, the post-marker accessor must NOT be a
    // `firstChild.nextSibling.nextSibling`-style walk from the parent.
    // That's the buggy pre-refactor pattern.
    //
    // Look for any `.firstChild.nextSibling.nextSibling` substring which
    // would indicate the walker did NOT re-anchor.
    assert!(
        !code.contains(".firstChild.nextSibling.nextSibling"),
        "After a marker pair, subsequent walks must chain off the close marker — \
         but found a `.firstChild.nextSibling.nextSibling` chain, which means the \
         walker is still indexing from the parent's firstChild and will land on \
         the wrong SSR DOM node. Got:\n{}",
        code
    );
}

#[test]
fn dom_regression_hydratable_two_consecutive_inserts_chain_off_each_other() {
    // Two consecutive expression-container children in hydratable mode:
    // pair 1 at positions 0–1, pair 2 at positions 2–3. The second
    // pair's open marker MUST be `<close-of-pair-1>.nextSibling`, and
    // its close marker MUST be `<open-of-pair-2>.nextSibling`-via-
    // getNextMarker. Anything that tries to walk the second pair from
    // `parent.firstChild.nextSibling.nextSibling` is wrong because pair
    // 1's close marker is not at a fixed offset from firstChild in the
    // SSR DOM (it depends on inserted content length).
    let code =
        transform_dom_hydratable_re(r#"const x = <div>{a}{b}</div>;"#);
    assert!(
        code.contains("getNextMarker"),
        "Expected getNextMarker for marker-pair walking. Got:\n{}",
        code
    );
    // Must not have a 3-deep nextSibling chain rooted at firstChild.
    assert!(
        !code.contains(".firstChild.nextSibling.nextSibling.nextSibling"),
        "A 3-deep nextSibling chain from firstChild indicates the walker is not \
         re-anchoring after each marker pair. Got:\n{}",
        code
    );
    // Must not have the bug where both pairs' opens chain off the parent.
    // We expect the second pair's open accessor to be a single-property
    // `.nextSibling` access on a destructured marker name.
    assert!(
        !code.contains(".firstChild.nextSibling.nextSibling"),
        "Walker did not re-anchor after first marker pair. Got:\n{}",
        code
    );
}

#[test]
fn dom_regression_hydratable_single_dynamic_child_no_marker() {
    // The single-dynamic-child fast path must still kick in under
    // hydratable mode: when `<nav>` has only a `<Show>` child and no
    // static siblings, the runtime can `insert(nav, Show(...))` without
    // any marker — the SSR output won't include marker comments either,
    // because the SSR transform also detects this case. Emitting a
    // marker here would invent a `<!$><!/>` pair that the SSR DOM does
    // not contain, and hydration would walk off the end.
    //
    // This is the navbar bug Bart was hitting on the live page.
    let code = transform_dom_hydratable_re(
        r#"const x = <nav><Show when={c}>hi</Show></nav>;"#,
    );
    assert!(
        !code.contains("<!$>"),
        "Single dynamic child must NOT emit `<!$>` marker (SSR has none). Got:\n{}",
        code
    );
    assert!(
        !code.contains("<!>"),
        "Single dynamic child must NOT emit `<!>` placeholder. Got:\n{}",
        code
    );
    assert!(
        !code.contains("getNextMarker"),
        "Single dynamic child must NOT pull in getNextMarker. Got:\n{}",
        code
    );
}

#[test]
fn ssr_regression_hydratable_spread_element_wraps_children_in_arrow() {
    // **Hydration-key drift bug.** When an SSR `ssrElement(tag, props,
    // children, true)` call passes `escape(...)` directly as the children
    // argument, JS evaluates argument expressions left-to-right BEFORE the
    // call — so every `ssrHydrationKey()` and `escape(createComponent(...))`
    // inside the children expression fires before `ssrElement` allocates the
    // parent's own hydration key. The DOM transform, in contrast, allocates
    // the parent first (`getNextElement(_tmpl)`) and only then the children
    // (via `insert(parent, () => local.children)` — also lazy on the DOM
    // side). The result is a mismatched count between SSR-emitted `data-hk`
    // values and DOM-side `getNextContextId()` calls, which surfaces as
    //     `Hydration Mismatch. Unable to find DOM nodes for hydration key`
    // for any element nested inside a spread-children element (the live
    // `/login` page hit it via `<Card>{form}</Card>`).
    //
    // The fix mirrors `babel-plugin-jsx-dom-expressions`
    // `ssr/element.js`: when `hydratable`, wrap the children expression in
    // `() => …` so the runtime evaluates it after `ssrHydrationKey()`.
    let code = transform_ssr_re(
        r#"const Card = (props) => <div {...props.rest}>{props.children}</div>;"#,
    );
    // Children must be a lazy `() => escape(...)`, NOT eager `escape(...)`.
    assert!(
        code.contains("() => escape"),
        "Hydratable ssrElement(...) must wrap children in `() => …`. Got:\n{}",
        code
    );
    // Sanity-check we still emit ssrElement at all.
    assert!(
        code.contains("ssrElement(\"div\""),
        "Spread-element SSR should still emit ssrElement(\"div\", ...). Got:\n{}",
        code
    );
}

#[test]
fn ssr_regression_non_hydratable_spread_element_does_not_wrap_children() {
    // The arrow-wrapping is only required for hydratable builds — its sole
    // purpose is to defer hydration-key allocation until after the parent
    // element's. In a non-hydratable build there is no `ssrHydrationKey()`
    // to fire, so wrapping just adds a needless function call. Babel
    // matches this: see the `hydratable ? arrow : children` ternary in
    // `babel-plugin-jsx-dom-expressions/src/ssr/element.js`.
    let options = TransformOptions {
        generate: GenerateMode::Ssr,
        hydratable: false,
        ..TransformOptions::solid_defaults()
    };
    let code = transform(
        r#"const Card = (props) => <div {...props.rest}>{props.children}</div>;"#,
        Some(options),
    )
    .code;
    assert!(
        !code.contains("() => escape"),
        "Non-hydratable ssrElement(...) must NOT wrap children in arrow. Got:\n{}",
        code
    );
    assert!(
        code.contains("escape("),
        "Non-hydratable ssrElement(...) should still escape its children. Got:\n{}",
        code
    );
}

#[test]
fn ssr_regression_hydratable_spread_element_emits_markers_around_dynamic_children() {
    // **`<select>` hydration-marker bug.** When a hydratable spread element has
    // *more than one* dynamic child, each dynamic slot must be wrapped with
    // `<!--$-->` / `<!--/-->` string literals so the emitted HTML carries the
    // comment markers that the DOM-side runtime's `getNextMarker` walks.
    //
    // Without these, the DOM template (e.g. `<select><!$><!/><!$><!/></select>`)
    // expects markers in the SSR HTML, but `getNextMarker` walks past the close
    // of the parent and returns `[null, …]`, surfacing as
    //     `Cannot read properties of null (reading 'nextSibling')`
    // — the failure mode hit by the live `/search` page's
    //     `<select>{cond && <option/>}<For/></select>`
    // with `props.spread` attached.
    //
    // Mirrors `babel-plugin-jsx-dom-expressions` `ssr/element.js`'s
    // `markers = hydratable && multi` branch: each ExpressionContainer or
    // Component child gets sandwiched between marker string literals; static
    // native children do not (they ARE template content, not dynamic slots).
    let code = transform_ssr_re(
        r#"const Test = (props) => (
  <select {...props.spread}>
    {props.placeholder && <option value="">{props.placeholder}</option>}
    <For each={props.options}>
      {(opt) => <option value={opt.value}>{opt.label}</option>}
    </For>
  </select>
);"#,
    );
    assert!(
        code.contains(r#""<!--$-->""#),
        "Multi-dynamic-child hydratable spread element must emit `<!--$-->` open markers. Got:\n{}",
        code
    );
    assert!(
        code.contains(r#""<!--/-->""#),
        "Multi-dynamic-child hydratable spread element must emit `<!--/-->` close markers. Got:\n{}",
        code
    );
    // Sanity: ssrElement is what we're decorating.
    assert!(
        code.contains("ssrElement(\"select\""),
        "Should still emit ssrElement(\"select\", …). Got:\n{}",
        code
    );
}

#[test]
fn ssr_regression_hydratable_spread_element_single_child_no_markers() {
    // Single-child case: NO markers. Babel only emits markers when
    // `multi = checkLength(children) > 1`. A single dynamic child needs no
    // separator — the DOM-side `getNextMarker` is only used when the template
    // includes multiple dynamic slots.
    let code = transform_ssr_re(
        r#"const Card = (props) => <div {...props.rest}>{props.children}</div>;"#,
    );
    assert!(
        !code.contains(r#""<!--$-->""#),
        "Single-dynamic-child spread element must NOT emit hydration markers. Got:\n{}",
        code
    );
}

#[test]
fn ssr_regression_non_hydratable_spread_element_emits_no_markers_even_multi() {
    // Markers only fire when `hydratable` — they exist purely for the DOM-side
    // hydration walker. In a non-hydratable build the runtime doesn't walk
    // markers (there is no DOM-side hydration), so emitting them would just
    // pollute the HTML output with stray comments.
    let options = TransformOptions {
        generate: GenerateMode::Ssr,
        hydratable: false,
        ..TransformOptions::solid_defaults()
    };
    let code = transform(
        r#"const Test = (props) => (
  <select {...props.spread}>
    {props.placeholder && <option/>}
    {props.other}
  </select>
);"#,
        Some(options),
    )
    .code;
    assert!(
        !code.contains(r#""<!--$-->""#),
        "Non-hydratable spread element must NOT emit hydration markers. Got:\n{}",
        code
    );
}

#[test]
fn ssr_regression_hydratable_spread_element_no_children_does_not_wrap_undefined() {
    // The "no children" sentinel `undefined` flows through to the runtime,
    // which checks for it explicitly and falls back to `props.children`
    // from the spread. Wrapping `undefined` in `() => undefined` would
    // defeat that detection — the runtime would see a function instead of
    // the `undefined` literal and treat it as actual children content,
    // duplicating whatever `props.children` resolves to.
    let code = transform_ssr_re(
        r#"const A = (props) => <a {...props.rest}/>;"#,
    );
    assert!(
        !code.contains("() => undefined"),
        "ssrElement(...)'s `undefined` no-children sentinel must NOT be \
         wrapped — the runtime checks for the literal. Got:\n{}",
        code
    );
    assert!(
        code.contains("undefined"),
        "ssrElement(...) for self-closing spread should still pass `undefined` \
         as the children argument. Got:\n{}",
        code
    );
}

