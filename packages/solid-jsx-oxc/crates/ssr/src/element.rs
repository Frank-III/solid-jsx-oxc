//! SSR element transform
//!
//! Transforms native HTML elements into SSR template strings.
//! Unlike DOM, we don't create DOM nodes - we build strings.

use oxc_ast::ast::{
    Argument, ArrayExpressionElement, Expression, FormalParameterKind, JSXAttribute,
    JSXAttributeItem, JSXAttributeName, JSXAttributeValue, JSXElement, PropertyKind, Statement,
};
use oxc_ast::{AstBuilder, NONE};
use oxc_span::{Span, SPAN};

use common::{
    constants::{ALIASES, CHILD_PROPERTIES, PROPERTIES, VOID_ELEMENTS},
    expression::escape_html,
    get_attr_name, is_dynamic, is_svg_element, TransformOptions,
};

use crate::component::{getter_return_expr, make_prop_key};

use crate::ir::{SSRContext, SSRResult};

/// Recursively transform a JSX child, dispatching to the right transformer.
///
/// This mirrors `SSRTransform::transform_node` but as a free function so it can
/// be passed as the child-transformer to `transform_component` from within
/// element processing (and pass *itself* as the recursive child-transformer
/// for nested components). Without this, components nested inside HTML
/// elements would lose all their props and children — see the regression test
/// `ssr_nested_component_in_html`.
pub(crate) fn transform_jsx_child<'a>(
    child: &oxc_ast::ast::JSXChild<'a>,
    context: &SSRContext<'a>,
    options: &TransformOptions<'a>,
) -> Option<SSRResult<'a>> {
    use oxc_ast::ast::JSXChild;
    match child {
        JSXChild::Element(el) => {
            let tag = common::get_tag_name(el);
            if common::is_component(&tag) {
                let inner =
                    |c: &JSXChild<'a>| transform_jsx_child(c, context, options);
                Some(crate::component::transform_component(
                    el, &tag, context, options, &inner,
                ))
            } else {
                Some(transform_element(el, &tag, context, options))
            }
        }
        JSXChild::Fragment(frag) => {
            let mut r = SSRResult::new();
            r.span = frag.span;
            for c in &frag.children {
                if let Some(child_r) = transform_jsx_child(c, context, options) {
                    r.merge(child_r);
                }
            }
            Some(r)
        }
        JSXChild::Text(text) => {
            // `transform_jsx_child` is called as the recursive child
            // transformer for component children (and fragment-of-children
            // contexts that bottom out at a component). Babel's
            // `transformComponentChildren` decodes JSX entities here:
            //
            //   const v = decode(trimWhitespace(path.node.extra.raw));
            //
            // and pushes `v` as a JS string literal — no HTML escape. The
            // string then flows through `createComponent({ children: [...] })`
            // and ultimately ends up in `escape()` if the component renders
            // it as text content, so passing the decoded character through
            // (rather than the entity form) is what keeps `&copy;` from
            // double-escaping into `&amp;copy;` at render time.
            let content = common::expression::trim_whitespace(&text.value);
            if content.is_empty() {
                None
            } else {
                let decoded = common::expression::decode_jsx_entities(&content);
                let mut r = SSRResult::new();
                r.span = text.span;
                r.push_static(&decoded);
                Some(r)
            }
        }
        JSXChild::ExpressionContainer(container) => {
            if let Some(expr) = container.expression.as_expression() {
                context.register_helper("escape");
                let mut r = SSRResult::new();
                r.span = container.span;
                r.push_dynamic(context.clone_expr(expr), false, false);
                Some(r)
            } else {
                None
            }
        }
        JSXChild::Spread(spread) => {
            let mut r = SSRResult::new();
            r.span = spread.span;
            context.register_helper("escape");
            r.push_dynamic(context.clone_expr(&spread.expression), false, false);
            Some(r)
        }
    }
}

/// Transform a native HTML/SVG element for SSR.
///
/// `top_level` controls whether this element gets its own `ssrHydrationKey()`
/// interpolation in the SSR template. **Only the root of each `ssr` template
/// chunk should emit one** — that's the element the DOM-side compiles into a
/// `getNextElement(_tmpl$)` call, which advances the hydration counter once.
/// Inner static elements are walked via `firstChild` / `nextSibling` on the
/// DOM side without any counter advance, so emitting `ssrHydrationKey()` on
/// them in SSR over-allocates context ids and drifts the SSR counter ahead of
/// the DOM counter — surfacing as `Hydration Mismatch. Unable to find DOM
/// nodes for hydration key …` for any element rendered after the drift point.
///
/// This mirrors `babel-plugin-jsx-dom-expressions`'s `ssr/element.js`, which
/// only emits `_$ssrHydrationKey()` for the outermost element passed to
/// `_$ssr(_tmpl$, hk()+attr, attr_only_for_inner_div, …)`.
pub fn transform_element<'a>(
    element: &JSXElement<'a>,
    tag_name: &str,
    context: &SSRContext<'a>,
    options: &TransformOptions<'a>,
) -> SSRResult<'a> {
    transform_element_impl(element, tag_name, context, options, true)
}

/// Internal entry point. Pass `top_level=false` for native elements that are
/// nested inside another native element's template chunk (their hydration is
/// handled by the parent's `getNextElement` + DOM-walking).
pub(crate) fn transform_element_nested<'a>(
    element: &JSXElement<'a>,
    tag_name: &str,
    context: &SSRContext<'a>,
    options: &TransformOptions<'a>,
) -> SSRResult<'a> {
    transform_element_impl(element, tag_name, context, options, false)
}

fn transform_element_impl<'a>(
    element: &JSXElement<'a>,
    tag_name: &str,
    context: &SSRContext<'a>,
    options: &TransformOptions<'a>,
    top_level: bool,
) -> SSRResult<'a> {
    let is_void = VOID_ELEMENTS.contains(tag_name);
    let is_script_or_style = tag_name == "script" || tag_name == "style";
    let ast = context.ast();

    let mut result = SSRResult::new();
    result.span = element.span;
    result.tag_name = Some(tag_name.to_string());
    result.skip_escape = is_script_or_style;

    // Check for spread attributes - need different handling
    let has_spread = element
        .opening_element
        .attributes
        .iter()
        .any(|a| matches!(a, JSXAttributeItem::SpreadAttribute(_)));

    if has_spread {
        return transform_element_with_spread(element, tag_name, context, options);
    }

    // Start the tag
    result.push_static(&format!("<{}", tag_name));

    // Add hydration key if needed — only on the **template root**. See the
    // doc comment on `transform_element` for the full rationale; emitting it
    // on nested static elements drifts the SSR counter ahead of the DOM
    // counter and breaks hydration.
    if top_level && context.hydratable && options.hydratable {
        context.register_helper("ssrHydrationKey");
        let callee = ast.expression_identifier(SPAN, "ssrHydrationKey");
        let expr = ast.expression_call(
            SPAN,
            callee,
            None::<oxc_ast::ast::TSTypeParameterInstantiation<'a>>,
            ast.vec(),
            false,
        );
        // Hydration key is interpolated INSIDE the opening tag, not in
        // text content — must NOT be wrapped with `<!--#-->…<!--/-->`
        // hydration markers (which would land between attributes and
        // produce malformed HTML the browser silently mis-parses, then
        // hydration mismatches and re-renders client-side).
        result.push_dynamic_with_marker(expr, false, true, false);
    }

    // Transform attributes
    transform_attributes(element, &mut result, context, options);

    // Close opening tag
    result.push_static(">");

    // Transform children (if not void element)
    if !is_void {
        transform_children(element, &mut result, context, options);
        result.push_static(&format!("</{}>", tag_name));
    }

    result
}

/// Transform element with spread attributes using ssrElement()
///
/// Output shape mirrors the Babel plugin:
///   ssrElement("tag", mergeProps(spread1, spread2, …, { staticProps, get dynProp() {…} }), childrenExpr, hydratable)
///
/// * Spreads stay as separate `mergeProps` arguments so their getters remain
///   lazy. Pre-merging via `{...rest, foo: 1}` would invoke every getter on
///   `rest` at object-literal time and lose reactivity (and lose the special
///   `children` accessor `<A>` from `@solidjs/router` relies on).
/// * Dynamic inline attrs become getter properties so they're evaluated
///   inside reactive contexts at render time.
/// * For self-closing elements with a spread, children flow through the
///   spread's `children` prop. Pass `undefined` so the dom-expressions
///   `ssrElement` runtime falls back to `props.children`.
fn transform_element_with_spread<'a>(
    element: &JSXElement<'a>,
    tag_name: &str,
    context: &SSRContext<'a>,
    options: &TransformOptions<'a>,
) -> SSRResult<'a> {
    context.register_helper("ssrElement");
    context.register_helper("escape");
    let ast = context.ast();
    let span = SPAN;

    let mut result = SSRResult::new();
    result.span = element.span;
    result.has_spread = true;

    let is_svg = is_svg_element(tag_name);

    let mut spreads: Vec<Expression<'a>> = Vec::new();
    let mut static_props: Vec<oxc_ast::ast::ObjectPropertyKind<'a>> = Vec::new();
    let mut dynamic_props: Vec<oxc_ast::ast::ObjectPropertyKind<'a>> = Vec::new();

    for attr in &element.opening_element.attributes {
        match attr {
            JSXAttributeItem::SpreadAttribute(spread) => {
                spreads.push(context.clone_expr(&spread.argument));
            }
            JSXAttributeItem::Attribute(attr) => {
                let key = get_attr_name(&attr.name);
                // Skip client-only attributes
                if key == "ref"
                    || key.starts_with("on")
                    || key.starts_with("use:")
                    || key.starts_with("prop:")
                {
                    continue;
                }

                let attr_name = if is_svg {
                    key.clone()
                } else {
                    ALIASES
                        .get(key.as_str())
                        .copied()
                        .unwrap_or(&key)
                        .to_string()
                };
                let prop_key = make_prop_key(ast, span, &attr_name);

                match &attr.value {
                    Some(JSXAttributeValue::StringLiteral(lit)) => {
                        // The dom-expressions runtime escapes attribute
                        // values when serialising — don't pre-escape here.
                        let value = ast.expression_string_literal(
                            span,
                            ast.allocator.alloc_str(&lit.value),
                            None,
                        );
                        static_props.push(ast.object_property_kind_object_property(
                            span,
                            PropertyKind::Init,
                            prop_key,
                            value,
                            false,
                            false,
                            false,
                        ));
                    }
                    Some(JSXAttributeValue::ExpressionContainer(container)) => {
                        if let Some(expr) = container.expression.as_expression() {
                            if is_dynamic(expr) {
                                let getter =
                                    getter_return_expr(ast, span, context.clone_expr(expr));
                                dynamic_props.push(ast.object_property_kind_object_property(
                                    span,
                                    PropertyKind::Get,
                                    prop_key,
                                    getter,
                                    false,
                                    false,
                                    false,
                                ));
                            } else {
                                static_props.push(ast.object_property_kind_object_property(
                                    span,
                                    PropertyKind::Init,
                                    prop_key,
                                    context.clone_expr(expr),
                                    false,
                                    false,
                                    false,
                                ));
                            }
                        }
                    }
                    None => {
                        let value = ast.expression_boolean_literal(span, true);
                        static_props.push(ast.object_property_kind_object_property(
                            span,
                            PropertyKind::Init,
                            prop_key,
                            value,
                            false,
                            false,
                            false,
                        ));
                    }
                    _ => {}
                }
            }
        }
    }

    let has_inline = !static_props.is_empty() || !dynamic_props.is_empty();
    let mut inline = ast.vec_with_capacity(static_props.len() + dynamic_props.len());
    for p in static_props {
        inline.push(p);
    }
    for p in dynamic_props {
        inline.push(p);
    }
    let inline_obj = ast.expression_object(span, inline);

    let props_expr = if spreads.is_empty() {
        inline_obj
    } else {
        context.register_helper("mergeProps");
        let callee = ast.expression_identifier(span, "mergeProps");
        let mut args = ast.vec_with_capacity(spreads.len() + if has_inline { 1 } else { 0 });
        for spread in spreads {
            args.push(Argument::from(spread));
        }
        if has_inline {
            args.push(Argument::from(inline_obj));
        }
        ast.expression_call(
            span,
            callee,
            None::<oxc_ast::ast::TSTypeParameterInstantiation<'a>>,
            args,
            false,
        )
    };

    // Build children. When the element has no JSX children but does have a
    // spread, children flow through the spread props — pass `undefined` so
    // the runtime reads `props.children`. Passing `null` would override that.
    //
    // Hydration-marker emission mirrors Babel's `ssr/element.js` spread-element
    // path (see `markers = hydratable && multi`). With *more than one*
    // meaningful child in a hydratable build, each *dynamic* child is
    // sandwiched between `"<!--$-->"` and `"<!--/-->"` string literals so the
    // emitted HTML carries the comment markers that the DOM-side runtime's
    // `getNextMarker` walks. Without these, `getNextMarker` walks past the
    // close of the parent and returns `[null, …]`, which surfaces as
    //     `Cannot read properties of null (reading 'nextSibling')`
    // — exactly the failure mode hit by `<select>{cond && <option/>}<For/></select>`
    // on the live `/search` page (Select had two dynamic children, no markers
    // in the SSR HTML, DOM template `<select><!$><!/><!$><!/></select>`).
    //
    // Static children (raw JSXText, native HTML element results that are
    // already complete `ssr\`…\`` templates) do NOT get markers — they ARE
    // the template content, not dynamic slots. Component results and bare
    // expression containers do.
    let children_expr = if element.children.is_empty() {
        ast.expression_identifier(span, "undefined")
    } else {
        // First pass: count meaningful children to decide whether we're in
        // the multi-child case. Same predicate as Babel's `checkLength`:
        //   - skip empty expression containers
        //   - skip text that is purely \r\n+spaces
        let mut meaningful = 0usize;
        for child in &element.children {
            match child {
                oxc_ast::ast::JSXChild::Text(text) => {
                    let raw = text.value.as_str();
                    if !raw.chars().all(|c| c.is_whitespace())
                        || raw.chars().any(|c| c == ' ')
                            && raw.chars().all(|c| c == ' ')
                    {
                        // Content text or non-newline whitespace
                        if !common::expression::trim_whitespace(&text.value).is_empty()
                            || raw.chars().all(|c| c == ' ')
                        {
                            meaningful += 1;
                        }
                    }
                }
                oxc_ast::ast::JSXChild::ExpressionContainer(c) => {
                    // Skip `{}` / `{/* comment */}`
                    if c.expression.as_expression().is_some() {
                        meaningful += 1;
                    }
                }
                oxc_ast::ast::JSXChild::Element(_) => meaningful += 1,
                _ => {}
            }
        }
        let markers = context.hydratable && meaningful > 1;

        let mut children: Vec<Expression<'a>> = Vec::new();
        let push_open_marker = |children: &mut Vec<Expression<'a>>| {
            children.push(ast.expression_string_literal(
                span,
                ast.allocator.alloc_str("<!--$-->"),
                None,
            ));
        };
        let push_close_marker = |children: &mut Vec<Expression<'a>>| {
            children.push(ast.expression_string_literal(
                span,
                ast.allocator.alloc_str("<!--/-->"),
                None,
            ));
        };

        for child in &element.children {
            match child {
                oxc_ast::ast::JSXChild::Text(text) => {
                    // Children for `ssrElement(tag, props, children, …)`. Babel's
                    // spread-element path uses
                    //   decode(trimWhitespace(path.node.extra.raw))
                    // with no escape — see `createElement` in
                    // `babel-plugin-jsx-dom-expressions/src/ssr/element.js`.
                    // The runtime's `resolveSSRNode` returns string children
                    // verbatim, so the decode step is what turns `&copy;`
                    // into `©` *before* we hand it off; otherwise it would
                    // survive as a literal `&copy;` token in the output.
                    let content = common::expression::trim_whitespace(&text.value);
                    if !content.is_empty() {
                        let decoded = common::expression::decode_jsx_entities(&content);
                        children.push(ast.expression_string_literal(
                            span,
                            ast.allocator.alloc_str(&decoded),
                            None,
                        ));
                    }
                }
                oxc_ast::ast::JSXChild::ExpressionContainer(container) => {
                    if let Some(expr) = container.expression.as_expression() {
                        if markers {
                            push_open_marker(&mut children);
                        }
                        let callee = ast.expression_identifier(span, "escape");
                        let mut args = ast.vec();
                        args.push(Argument::from(context.clone_expr(expr)));
                        children.push(ast.expression_call(
                            span,
                            callee,
                            None::<oxc_ast::ast::TSTypeParameterInstantiation<'a>>,
                            args,
                            false,
                        ));
                        if markers {
                            push_close_marker(&mut children);
                        }
                    }
                }
                oxc_ast::ast::JSXChild::Element(child_elem) => {
                    // Recursively transform child element - check if component or native
                    let child_tag = common::get_tag_name(child_elem);
                    let is_component = common::is_component(&child_tag);
                    let child_result = if is_component {
                        let child_transformer = |c: &oxc_ast::ast::JSXChild<'a>| {
                            transform_jsx_child(c, context, options)
                        };
                        crate::component::transform_component(
                            child_elem,
                            &child_tag,
                            context,
                            options,
                            &child_transformer,
                        )
                    } else {
                        transform_element(child_elem, &child_tag, context, options)
                    };

                    // Components are dynamic; native static elements are
                    // their own self-contained `ssr\`…\`` templates and
                    // don't need wrapping markers. Native elements with a
                    // spread (`has_spread`) are a third case that Babel
                    // skips markers for (`!child.spreadElement`) — the
                    // child's `ssrElement(...)` call already accounts for
                    // its own hydration wiring.
                    let needs_markers = markers && is_component;
                    if needs_markers {
                        push_open_marker(&mut children);
                    }
                    children.push(child_result.to_ssr_expression(context, context.hydratable));
                    if needs_markers {
                        push_close_marker(&mut children);
                    }
                }
                _ => {}
            }
        }

        if children.len() == 1 {
            children
                .pop()
                .unwrap_or_else(|| ast.expression_null_literal(span))
        } else if children.is_empty() {
            ast.expression_null_literal(span)
        } else {
            let mut elements = ast.vec_with_capacity(children.len());
            for expr in children {
                elements.push(ArrayExpressionElement::from(expr));
            }
            ast.expression_array(span, elements)
        }
    };

    // For spread, we generate: ssrElement("tag", props, children, needsHydrationKey)
    //
    // In hydratable mode, wrap a non-empty `children` in an arrow function so
    // it evaluates *after* `ssrElement` has allocated its own
    // `ssrHydrationKey()` for this element. Without the wrapper, the JS
    // call-argument evaluation order eagerly evaluates the children
    // expression first, draining several `getNextContextId` counts (every
    // nested template literal + child component) before the parent's hk is
    // allocated. SSR then writes parent hk = `<ctx>.<N>` while DOM (which
    // *does* allocate the parent first) walks `<ctx>.0`, producing
    //
    //     "Hydration Mismatch. Unable to find DOM nodes for hydration key: …"
    //
    // exactly the failure mode that surfaced on the live `/login` page after
    // the marker-pair walker fix. Mirrors `babel-plugin-jsx-dom-expressions`
    // `ssr/element.js` line ~597:
    //
    //     hydratable
    //       ? t.arrowFunctionExpression([], …)
    //       : children
    //
    // We don't wrap when `children_expr` is the bare `undefined` identifier
    // (the no-children spread fallback that flows through `props.children`)
    // — the runtime checks for that sentinel.
    let children_arg = if context.hydratable && !is_undefined_identifier(&children_expr) {
        wrap_in_arrow_zero(ast, span, children_expr)
    } else {
        children_expr
    };
    let callee = ast.expression_identifier(span, "ssrElement");
    let mut args = ast.vec();
    args.push(Argument::from(ast.expression_string_literal(
        span,
        ast.allocator.alloc_str(tag_name),
        None,
    )));
    args.push(Argument::from(props_expr));
    args.push(Argument::from(children_arg));
    args.push(Argument::from(ast.expression_boolean_literal(
        span,
        context.hydratable && options.hydratable,
    )));
    let call = ast.expression_call(
        span,
        callee,
        None::<oxc_ast::ast::TSTypeParameterInstantiation<'a>>,
        args,
        false,
    );
    result.push_dynamic(call, false, true);

    result
}

/// `() => expr` — zero-param arrow with expression body. Used to defer
/// evaluation of an `ssrElement` `children` argument until after the runtime
/// has allocated the element's own `ssrHydrationKey()`.
fn wrap_in_arrow_zero<'a>(
    ast: AstBuilder<'a>,
    span: Span,
    expr: Expression<'a>,
) -> Expression<'a> {
    let params = ast.alloc_formal_parameters(
        span,
        FormalParameterKind::ArrowFormalParameters,
        ast.vec(),
        NONE,
    );
    let mut statements = ast.vec_with_capacity(1);
    statements.push(Statement::ExpressionStatement(
        ast.alloc_expression_statement(span, expr),
    ));
    let body = ast.alloc_function_body(span, ast.vec(), statements);
    ast.expression_arrow_function(span, true, false, NONE, params, NONE, body)
}

/// True for the bare `undefined` identifier we emit as the no-children
/// sentinel in `ssrElement(...)`. We never wrap that in an arrow because the
/// runtime checks for it explicitly to fall back to `props.children`.
fn is_undefined_identifier<'a>(expr: &Expression<'a>) -> bool {
    matches!(expr, Expression::Identifier(id) if id.name == "undefined")
}

/// Transform element attributes for SSR
fn transform_attributes<'a>(
    element: &JSXElement<'a>,
    result: &mut SSRResult<'a>,
    context: &SSRContext<'a>,
    options: &TransformOptions<'a>,
) {
    let tag_name = result.tag_name.as_deref().unwrap_or("");
    let is_svg = is_svg_element(tag_name);

    // Pre-scan: when both `class`/`className` and `classList` are present on
    // the same element, browsers keep only the *first* `class` attribute and
    // discard subsequent duplicates. Emitting two attributes (one for the
    // static class, one for the classList result) silently drops half the
    // classes from the SSR HTML — and DOM-side hydration then re-applies them
    // at runtime, producing a hydration mismatch and a visible flicker.
    //
    // Babel's `babel-plugin-jsx-dom-expressions` inlines both into a single
    // `class="…"` interpolation at compile time. We do the same: when we see
    // the pair, emit a combined `class="<merge>"` attribute and skip the
    // individual class / classList code paths.
    let (has_class, has_class_list) = {
        let mut hc = false;
        let mut hcl = false;
        for attr in &element.opening_element.attributes {
            if let JSXAttributeItem::Attribute(attr) = attr {
                let k = get_attr_name(&attr.name);
                if k == "class" || k == "className" {
                    hc = true;
                } else if k == "classList" {
                    hcl = true;
                }
            }
        }
        (hc, hcl)
    };
    let merge_class_and_classlist = has_class && has_class_list;

    let mut emitted_combined_class = false;

    for attr in &element.opening_element.attributes {
        if let JSXAttributeItem::Attribute(attr) = attr {
            let k = get_attr_name(&attr.name);

            if merge_class_and_classlist
                && (k == "class" || k == "className" || k == "classList")
            {
                if !emitted_combined_class {
                    emit_combined_class_attribute(
                        &element.opening_element.attributes,
                        result,
                        context,
                    );
                    emitted_combined_class = true;
                }
                continue;
            }

            transform_attribute(attr, result, context, options, is_svg);
        }
    }
}

/// Emit a single `class="..."` attribute that merges the element's static
/// `class` / `className` and dynamic `classList` into one interpolation.
///
/// Matches Babel's behavior of inlining both at compile time so the SSR HTML
/// has exactly one `class` attribute. The combined value is built as:
///
///   * pure-static class only          → no merge needed (handled in caller).
///   * static `class="x"` + classList  → `"x " + ssrClassList(obj)`
///   * dynamic `class={cls}` + classList → `escape(cls, true) + " " + ssrClassList(obj)`
///
/// In every case we go through `escape(..., true)` for the dynamic side to
/// preserve attribute-context escaping, and the dom-expressions runtime's
/// `ssrClassList` already returns an attribute-safe string.
fn emit_combined_class_attribute<'a>(
    attributes: &oxc_allocator::Vec<'a, JSXAttributeItem<'a>>,
    result: &mut SSRResult<'a>,
    context: &SSRContext<'a>,
) {
    let ast = context.ast();
    context.register_helper("ssrClassList");
    context.register_helper("escape");

    // Find the two relevant attribute values.
    let mut class_attr: Option<&JSXAttribute<'a>> = None;
    let mut class_list_attr: Option<&JSXAttribute<'a>> = None;
    for attr in attributes {
        if let JSXAttributeItem::Attribute(attr) = attr {
            let k = get_attr_name(&attr.name);
            if (k == "class" || k == "className") && class_attr.is_none() {
                class_attr = Some(attr);
            } else if k == "classList" && class_list_attr.is_none() {
                class_list_attr = Some(attr);
            }
        }
    }

    result.push_static(" class=\"");

    // Class side (string-or-dynamic).
    if let Some(class_attr) = class_attr {
        match &class_attr.value {
            Some(JSXAttributeValue::StringLiteral(lit)) => {
                let escaped = escape_html(&lit.value, true);
                // Trailing space separates the static class from the
                // classList output. Babel emits the same — duplicate spaces
                // are harmless because the browser tokenizes `class` on
                // whitespace.
                result.push_static(&format!("{} ", escaped));
            }
            Some(JSXAttributeValue::ExpressionContainer(container)) => {
                if let Some(expr) = container.expression.as_expression() {
                    result.push_dynamic(context.clone_expr(expr), true, false);
                    result.push_static(" ");
                }
            }
            _ => {}
        }
    }

    // ClassList side — always goes through `ssrClassList(...)`.
    if let Some(class_list_attr) = class_list_attr {
        if let Some(JSXAttributeValue::ExpressionContainer(container)) = &class_list_attr.value {
            if let Some(expr) = container.expression.as_expression() {
                let callee = ast.expression_identifier(SPAN, "ssrClassList");
                let mut args = ast.vec();
                args.push(Argument::from(context.clone_expr(expr)));
                let call = ast.expression_call(
                    SPAN,
                    callee,
                    None::<oxc_ast::ast::TSTypeParameterInstantiation<'a>>,
                    args,
                    false,
                );
                // skip_escape: ssrClassList already returns an
                // attribute-safe string, so we don't double-escape.
                result.push_dynamic_with_marker(call, false, true, false);
            }
        }
    }

    result.push_static("\"");
}

/// Transform a single attribute for SSR
fn transform_attribute<'a>(
    attr: &JSXAttribute<'a>,
    result: &mut SSRResult<'a>,
    context: &SSRContext<'a>,
    _options: &TransformOptions<'a>,
    is_svg: bool,
) {
    let ast = context.ast();
    let key = get_attr_name(&attr.name);

    // Skip client-only attributes
    if key == "ref" || key.starts_with("on") || key.starts_with("use:") || key.starts_with("prop:")
    {
        return;
    }

    // Handle child properties (innerHTML, textContent)
    if CHILD_PROPERTIES.contains(key.as_str()) {
        // These are handled in children transform
        return;
    }

    // Get the attribute name (handle aliases like className -> class)
    let attr_name = if is_svg {
        key.clone()
    } else {
        ALIASES
            .get(key.as_str())
            .copied()
            .unwrap_or(&key)
            .to_string()
    };

    match &attr.value {
        // Static string value
        Some(JSXAttributeValue::StringLiteral(lit)) => {
            let escaped = escape_html(&lit.value, true);
            result.push_static(&format!(" {}=\"{}\"", attr_name, escaped));
        }

        // Dynamic value
        Some(JSXAttributeValue::ExpressionContainer(container)) => {
            if let Some(expr) = container.expression.as_expression() {
                let expr = context.clone_expr(expr);

                // Handle special attributes
                if key == "style" {
                    context.register_helper("ssrStyle");
                    result.push_static(&format!(" {}=\"", attr_name));
                    let callee = ast.expression_identifier(SPAN, "ssrStyle");
                    let mut args = ast.vec();
                    args.push(Argument::from(expr));
                    // Inside attribute value — no hydration markers.
                    result.push_dynamic_with_marker(
                        ast.expression_call(
                            SPAN,
                            callee,
                            None::<oxc_ast::ast::TSTypeParameterInstantiation<'a>>,
                            args,
                            false,
                        ),
                        false,
                        true,
                        false,
                    );
                    result.push_static("\"");
                } else if key == "class" || key == "className" {
                    context.register_helper("escape");
                    result.push_static(&format!(" {}=\"", attr_name));
                    result.push_dynamic(expr, true, false);
                    result.push_static("\"");
                } else if key == "classList" {
                    context.register_helper("ssrClassList");
                    result.push_static(" class=\"");
                    let callee = ast.expression_identifier(SPAN, "ssrClassList");
                    let mut args = ast.vec();
                    args.push(Argument::from(expr));
                    // Inside attribute value — no hydration markers.
                    result.push_dynamic_with_marker(
                        ast.expression_call(
                            SPAN,
                            callee,
                            None::<oxc_ast::ast::TSTypeParameterInstantiation<'a>>,
                            args,
                            false,
                        ),
                        false,
                        true,
                        false,
                    );
                    result.push_static("\"");
                } else if PROPERTIES.contains(key.as_str()) {
                    // Boolean attributes
                    context.register_helper("ssrAttribute");
                    let callee = ast.expression_identifier(SPAN, "ssrAttribute");
                    let mut args = ast.vec();
                    args.push(Argument::from(ast.expression_string_literal(
                        SPAN,
                        ast.allocator.alloc_str(&attr_name),
                        None,
                    )));
                    args.push(Argument::from(expr));
                    args.push(Argument::from(ast.expression_boolean_literal(SPAN, true)));
                    // Emitted inside the opening tag — no hydration markers.
                    result.push_dynamic_with_marker(
                        ast.expression_call(
                            SPAN,
                            callee,
                            None::<oxc_ast::ast::TSTypeParameterInstantiation<'a>>,
                            args,
                            false,
                        ),
                        false,
                        true,
                        false,
                    );
                } else {
                    // Regular attribute
                    context.register_helper("escape");
                    result.push_static(&format!(" {}=\"", attr_name));
                    result.push_dynamic(expr, true, false);
                    result.push_static("\"");
                }
            }
        }

        // Boolean attribute (no value)
        None => {
            result.push_static(&format!(" {}", attr_name));
        }

        _ => {}
    }
}

/// Transform element children for SSR
fn transform_children<'a>(
    element: &JSXElement<'a>,
    result: &mut SSRResult<'a>,
    context: &SSRContext<'a>,
    options: &TransformOptions<'a>,
) {
    // Check for innerHTML/textContent in attributes first
    for attr in &element.opening_element.attributes {
        if let JSXAttributeItem::Attribute(attr) = attr {
            let key = match &attr.name {
                JSXAttributeName::Identifier(id) => id.name.as_str(),
                _ => continue,
            };

            if key == "innerHTML" {
                if let Some(JSXAttributeValue::ExpressionContainer(container)) = &attr.value {
                    if let Some(expr) = container.expression.as_expression() {
                        // innerHTML - don't escape
                        result.push_dynamic(context.clone_expr(expr), false, true);
                        return;
                    }
                }
            } else if key == "textContent" || key == "innerText" {
                if let Some(JSXAttributeValue::ExpressionContainer(container)) = &attr.value {
                    if let Some(expr) = container.expression.as_expression() {
                        context.register_helper("escape");
                        result.push_dynamic(context.clone_expr(expr), false, false);
                        return;
                    }
                }
            }
        }
    }

    // Process children
    let skip_escape = result.skip_escape;
    let multi = count_meaningful_children(&element.children) > 1;
    let hydratable = context.hydratable && options.hydratable;
    process_jsx_children(
        &element.children,
        result,
        skip_escape,
        context,
        options,
        multi,
        hydratable,
    );
}

/// Count "meaningful" children for babel's multi-children rule.
///
/// Mirrors `checkLength` in `babel-plugin-jsx-dom-expressions/src/shared/utils.js`:
/// whitespace-only text nodes don't count. Fragments are expanded inline so
/// their meaningful children count toward the parent's total.
fn count_meaningful_children<'a>(
    children: &oxc_allocator::Vec<'a, oxc_ast::ast::JSXChild<'a>>,
) -> usize {
    let mut n = 0;
    for c in children {
        match c {
            oxc_ast::ast::JSXChild::Text(t) => {
                if !common::expression::trim_whitespace(&t.value).is_empty() {
                    n += 1;
                }
            }
            oxc_ast::ast::JSXChild::Fragment(frag) => {
                n += count_meaningful_children(&frag.children);
            }
            _ => n += 1,
        }
    }
    n
}

/// Process a list of JSX children, appending to the result.
///
/// `multi` and `hydratable` come from the **parent element's** child count and
/// hydration mode. Babel only wraps dynamic children with `<!--$-->`/`<!--/-->`
/// when both are true (`markers = hydratable && multi`); a single dynamic
/// child stays unwrapped because the runtime locates it by walking the
/// surrounding element instead of by marker comments. Mismatching this rule
/// produces marker comments the runtime can't find, which falls back to a
/// full client-side re-render (the visible "HTML flashing" symptom).
fn process_jsx_children<'a>(
    children: &oxc_allocator::Vec<'a, oxc_ast::ast::JSXChild<'a>>,
    result: &mut SSRResult<'a>,
    skip_escape: bool,
    context: &SSRContext<'a>,
    options: &TransformOptions<'a>,
    multi: bool,
    hydratable: bool,
) {
    let wrap = multi && hydratable;
    for child in children {
        match child {
            oxc_ast::ast::JSXChild::Text(text) => {
                let content = common::expression::trim_whitespace(&text.value);
                if !content.is_empty() {
                    // Babel's native-element JSX-text path is just
                    // `appendToTemplate(template, trimWhitespace(node.extra.raw))`
                    // — no escape, no entity decode. Entity-form text like
                    // `&copy;` is meant to flow into the SSR template
                    // verbatim and be decoded by the browser's HTML parser
                    // at render time. Calling `escape_html` here was a bug:
                    // it doubled `&copy;` into `&amp;copy;`, which the
                    // browser then renders as the literal text `&copy;`.
                    // The `skip_escape` parameter is preserved for symmetry
                    // with the dynamic-child path but does not change the
                    // text-node behavior (we never escape text either way).
                    let _ = skip_escape;
                    result.push_static(&content);
                }
            }

            oxc_ast::ast::JSXChild::Element(child_elem) => {
                let child_tag = common::get_tag_name(child_elem);
                let is_comp = common::is_component(&child_tag);
                let child_result = if is_comp {
                    // Recursive child transformer — handles arbitrarily deep
                    // nesting of components, elements, fragments, text and
                    // expression containers.
                    let child_transformer =
                        |c: &oxc_ast::ast::JSXChild<'a>| transform_jsx_child(c, context, options);
                    crate::component::transform_component(
                        child_elem,
                        &child_tag,
                        context,
                        options,
                        &child_transformer,
                    )
                } else {
                    // Native element nested directly inside this element's
                    // template chunk — its bytes are merged into the parent
                    // template via `result.merge`. The DOM-side reaches it by
                    // walking from the parent's `getNextElement` result; no
                    // separate counter advance happens, so SSR must NOT emit
                    // its own `ssrHydrationKey()` here either.
                    transform_element_nested(child_elem, &child_tag, context, options)
                };
                // Component children emit dynamic expressions (a
                // `createComponent(...)` or `escape(...)` call) and the
                // runtime's hydration walk uses `<!--$-->`/`<!--/-->`
                // markers to find them in the SSR DOM — so they DO take
                // markers in a hydratable + multi-child parent.
                //
                // Native-element children with a spread (`<input
                // {...rest}/>`) emit a bare `ssrElement(...)` call whose
                // runtime output is the element's HTML directly. The
                // client-side compiler treats the same JSX as a static
                // template chunk inside the parent (`<label><input
                // ...>`), so there's no hydration walk that expects
                // markers around it. Wrapping it would put `<!--$-->`
                // *between* the parent's opening tag and the actual
                // element — `parent.firstChild` post-hydration would
                // then resolve to the comment node, not the element,
                // and refs/listeners would silently bind to the
                // comment.
                //
                // Babel's reference plugin uses `!child.spreadElement`
                // in `src/ssr/element.js:509,514` to skip marker
                // emission for the spread case. Since spread elements
                // are by definition `!is_comp`, the rule simplifies to
                // "wrap markers iff component child". Static native
                // elements (no spread) merge static template bytes via
                // `result.merge` and never need markers either way.
                if wrap && is_comp {
                    result.push_static("<!--$-->");
                    result.merge(child_result);
                    result.push_static("<!--/-->");
                } else {
                    result.merge(child_result);
                }
            }

            oxc_ast::ast::JSXChild::ExpressionContainer(container) => {
                if let Some(expr) = container.expression.as_expression() {
                    let expr = context.clone_expr(expr);

                    if skip_escape {
                        // Inside script/style - don't escape
                        if wrap {
                            result.push_static("<!--$-->");
                            result.push_dynamic(expr, false, true);
                            result.push_static("<!--/-->");
                        } else {
                            result.push_dynamic(expr, false, true);
                        }
                    } else {
                        // Normal content - escape
                        context.register_helper("escape");
                        if wrap {
                            result.push_static("<!--$-->");
                            result.push_dynamic(expr, false, false);
                            result.push_static("<!--/-->");
                        } else {
                            result.push_dynamic(expr, false, false);
                        }
                    }
                }
            }

            oxc_ast::ast::JSXChild::Fragment(fragment) => {
                // Recursively process fragment children with same escape
                // settings. Pass `multi` through unchanged: babel treats
                // fragment expansion as part of the surrounding parent's
                // child set when deciding whether to mark a dynamic child.
                process_jsx_children(
                    &fragment.children,
                    result,
                    skip_escape,
                    context,
                    options,
                    multi,
                    hydratable,
                );
            }

            _ => {}
        }
    }
}
