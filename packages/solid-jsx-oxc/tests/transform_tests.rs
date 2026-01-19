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
    // Component nested multiple levels deep
    let code = transform_dom(r#"<div><main><Counter /></main></div>"#);

    // Template should have placeholder in the nested element
    assert!(code.contains("<div><main><!></main></div>"));

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

    // Template should have placeholder
    assert!(code.contains("<div><section><article><!></article></section></div>"));

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
