//! Component transform
//! Handles <MyComponent /> -> createComponent(MyComponent, {...})

use oxc_ast::ast::{JSXAttributeItem, JSXAttributeName, JSXAttributeValue, JSXChild, JSXElement};

use common::{
    expr_to_string, find_prop, get_children_callback, is_built_in, is_dynamic, TransformOptions,
};

use crate::ir::{BlockContext, ChildTransformer, Expr, TransformResult};

fn build_child_output(result: &TransformResult, context: &BlockContext) -> Option<String> {
    // Handle fragment with mixed children (array output)
    if !result.child_codes.is_empty() {
        return Some(format!("[{}]", result.child_codes.join(", ")));
    }

    // Handle text-only result - just return the string literal
    if result.text && !result.template.is_empty() {
        return Some(format!("\"{}\"", result.template));
    }

    // If there's a template, we need to clone it
    if !result.template.is_empty() && !result.skip_template {
        // Push template and get variable name
        let tmpl_idx = context.push_template(result.template.clone(), result.is_svg);
        let tmpl_var = format!("_tmpl${}", tmpl_idx + 1);

        // Use the generated element ID when available (matches expression wiring).
        // Fall back to a local _el$ when the element didn't require a stable ID.
        let elem_var = result.id.clone().unwrap_or_else(|| "_el$".to_string());

        let mut code = String::new();
        code.push_str("(() => { ");
        code.push_str(&format!("const {} = {}.cloneNode(true); ", elem_var, tmpl_var));

        // Add declarations (element walking for nested elements)
        for decl in &result.declarations {
            code.push_str(&format!("const {} = {}; ", decl.name, decl.init));
        }

        // Add expressions (effects, inserts, etc.)
        for expr in &result.exprs {
            code.push_str(&format!("{}; ", expr.code));
        }

        // Add dynamic bindings
        for binding in &result.dynamics {
            context.register_helper("effect");
            if binding.key == "style" {
                context.register_helper("style");
            } else if binding.key == "classList" {
                context.register_helper("classList");
            } else {
                context.register_helper("setAttribute");
            }
            let setter = crate::template::generate_set_attr(binding);
            code.push_str(&format!("effect(() => {}); ", setter));
        }

        // Add post expressions
        for expr in &result.post_exprs {
            code.push_str(&format!("{}; ", expr.code));
        }

        code.push_str(&format!("return {}; }})()", elem_var));
        return Some(code);
    }

    // Just expressions (like a component call or fragment expression)
    if !result.exprs.is_empty() {
        let expr_code = result
            .exprs
            .iter()
            .map(|e| e.code.clone())
            .collect::<Vec<_>>()
            .join(", ");

        if result.needs_memo {
            context.register_helper("memo");
            return Some(format!("memo({})", expr_code));
        }

        return Some(expr_code);
    }

    None
}

/// Transform a component element
pub fn transform_component<'a, 'b>(
    element: &JSXElement<'a>,
    tag_name: &str,
    context: &BlockContext,
    options: &TransformOptions<'a>,
    transform_child: ChildTransformer<'a, 'b>,
) -> TransformResult {
    let mut result = TransformResult::default();

    // Check if this is a built-in (For, Show, etc.)
    if is_built_in(tag_name) {
        return transform_builtin(element, tag_name, context, options, transform_child);
    }

    context.register_helper("createComponent");

    // Build props object
    let props = build_props(element, context, options, transform_child);

    // Generate createComponent call
    result.exprs.push(Expr {
        code: format!("createComponent({}, {})", tag_name, props),
    });

    result
}

/// Transform built-in control flow components
fn transform_builtin<'a, 'b>(
    element: &JSXElement<'a>,
    tag_name: &str,
    context: &BlockContext,
    options: &TransformOptions<'a>,
    transform_child: ChildTransformer<'a, 'b>,
) -> TransformResult {
    let mut result = TransformResult::default();

    match tag_name {
        "For" => transform_for(element, &mut result, context, transform_child),
        "Show" => transform_show(element, &mut result, context, transform_child),
        "Switch" => transform_switch(element, &mut result, context, transform_child),
        "Match" => transform_match(element, &mut result, context, transform_child),
        "Index" => transform_index(element, &mut result, context, transform_child),
        "Suspense" => transform_suspense(element, &mut result, context, transform_child),
        "Portal" => transform_portal(element, &mut result, context, transform_child),
        "Dynamic" => transform_dynamic(element, &mut result, context, options, transform_child),
        "ErrorBoundary" => transform_error_boundary(element, &mut result, context, transform_child),
        _ => {
            // Fallback to regular component transform
            context.register_helper("createComponent");
            result.exprs.push(Expr {
                code: format!("createComponent({}, {{}})", tag_name),
            });
        }
    }

    result
}

/// Helper to get a prop expression
fn get_prop_expr<'a>(element: &JSXElement<'a>, name: &str) -> String {
    find_prop(element, name)
        .and_then(|attr| attr.value.as_ref())
        .and_then(|v| match v {
            JSXAttributeValue::ExpressionContainer(c) => c.expression.as_expression(),
            _ => None,
        })
        .map(|e| expr_to_string(e))
        .unwrap_or_else(|| "undefined".to_string())
}

/// Transform <For each={...}>{item => ...}</For>
fn transform_for<'a, 'b>(
    element: &JSXElement<'a>,
    result: &mut TransformResult,
    context: &BlockContext,
    _transform_child: ChildTransformer<'a, 'b>,
) {
    context.register_helper("createComponent");
    // Note: For is expected to be imported by user from solid-js, not added here

    let each_expr = get_prop_expr(element, "each");
    let children = get_children_callback(element);

    result.exprs.push(Expr {
        code: format!(
            "createComponent(For, {{ each: {}, children: {} }})",
            each_expr, children
        ),
    });
}

/// Transform <Show when={...} fallback={...}>...</Show>
fn transform_show<'a, 'b>(
    element: &JSXElement<'a>,
    result: &mut TransformResult,
    context: &BlockContext,
    transform_child: ChildTransformer<'a, 'b>,
) {
    context.register_helper("createComponent");
    // Note: Show is expected to be imported by user from solid-js

    let when_expr = get_prop_expr(element, "when");
    let fallback_expr = get_prop_expr(element, "fallback");
    let children = get_children_expr_transformed(element, context, transform_child);

    result.exprs.push(Expr {
        code: format!(
            "createComponent(Show, {{ when: {}, fallback: {}, get children() {{ return {}; }} }})",
            when_expr, fallback_expr, children
        ),
    });
}

/// Transform <Switch>...</Switch>
fn transform_switch<'a, 'b>(
    element: &JSXElement<'a>,
    result: &mut TransformResult,
    context: &BlockContext,
    transform_child: ChildTransformer<'a, 'b>,
) {
    context.register_helper("createComponent");
    // Note: Switch is expected to be imported by user from solid-js

    let children = get_children_expr_transformed(element, context, transform_child);

    result.exprs.push(Expr {
        code: format!(
            "createComponent(Switch, {{ get children() {{ return {}; }} }})",
            children
        ),
    });
}

/// Transform <Match when={...}>...</Match>
fn transform_match<'a, 'b>(
    element: &JSXElement<'a>,
    result: &mut TransformResult,
    context: &BlockContext,
    transform_child: ChildTransformer<'a, 'b>,
) {
    context.register_helper("createComponent");
    // Note: Match is expected to be imported by user from solid-js

    let when_expr = get_prop_expr(element, "when");
    let children = get_children_expr_transformed(element, context, transform_child);

    result.exprs.push(Expr {
        code: format!(
            "createComponent(Match, {{ when: {}, get children() {{ return {}; }} }})",
            when_expr, children
        ),
    });
}

/// Transform <Index each={...}>{(item, index) => ...}</Index>
fn transform_index<'a, 'b>(
    element: &JSXElement<'a>,
    result: &mut TransformResult,
    context: &BlockContext,
    _transform_child: ChildTransformer<'a, 'b>,
) {
    context.register_helper("createComponent");
    // Note: Index is expected to be imported by user from solid-js

    let each_expr = get_prop_expr(element, "each");
    let children = get_children_callback(element);

    result.exprs.push(Expr {
        code: format!(
            "createComponent(Index, {{ each: {}, children: {} }})",
            each_expr, children
        ),
    });
}

/// Transform <Suspense fallback={...}>...</Suspense>
fn transform_suspense<'a, 'b>(
    element: &JSXElement<'a>,
    result: &mut TransformResult,
    context: &BlockContext,
    transform_child: ChildTransformer<'a, 'b>,
) {
    context.register_helper("createComponent");
    // Note: Suspense is expected to be imported by user from solid-js

    let fallback_expr = get_prop_expr(element, "fallback");
    let children = get_children_expr_transformed(element, context, transform_child);

    result.exprs.push(Expr {
        code: format!(
            "createComponent(Suspense, {{ fallback: {}, get children() {{ return {}; }} }})",
            fallback_expr, children
        ),
    });
}

/// Transform <Portal mount={...}>...</Portal>
fn transform_portal<'a, 'b>(
    element: &JSXElement<'a>,
    result: &mut TransformResult,
    context: &BlockContext,
    transform_child: ChildTransformer<'a, 'b>,
) {
    context.register_helper("createComponent");
    // Note: Portal is expected to be imported by user from solid-js/web

    let mount_expr = get_prop_expr(element, "mount");
    let children = get_children_expr_transformed(element, context, transform_child);

    result.exprs.push(Expr {
        code: format!(
            "createComponent(Portal, {{ mount: {}, get children() {{ return {}; }} }})",
            mount_expr, children
        ),
    });
}

/// Transform <Dynamic component={...} {...props} />
fn transform_dynamic<'a, 'b>(
    element: &JSXElement<'a>,
    result: &mut TransformResult,
    context: &BlockContext,
    options: &TransformOptions<'a>,
    transform_child: ChildTransformer<'a, 'b>,
) {
    context.register_helper("createComponent");
    // Note: Dynamic is expected to be imported by user from solid-js/web

    let component_expr = get_prop_expr(element, "component");
    let props = build_props(element, context, options, transform_child);

    result.exprs.push(Expr {
        code: format!(
            "createComponent(Dynamic, {{ component: {}, ...{} }})",
            component_expr, props
        ),
    });
}

/// Transform <ErrorBoundary fallback={...}>...</ErrorBoundary>
fn transform_error_boundary<'a, 'b>(
    element: &JSXElement<'a>,
    result: &mut TransformResult,
    context: &BlockContext,
    transform_child: ChildTransformer<'a, 'b>,
) {
    context.register_helper("createComponent");
    // Note: ErrorBoundary is expected to be imported by user from solid-js

    let fallback_expr = get_prop_expr(element, "fallback");
    let children = get_children_expr_transformed(element, context, transform_child);

    result.exprs.push(Expr {
        code: format!(
            "createComponent(ErrorBoundary, {{ fallback: {}, get children() {{ return {}; }} }})",
            fallback_expr, children
        ),
    });
}

/// Build props object for a component
fn build_props<'a, 'b>(
    element: &JSXElement<'a>,
    context: &BlockContext,
    _options: &TransformOptions<'a>,
    transform_child: ChildTransformer<'a, 'b>,
) -> String {
    fn is_valid_prop_identifier(key: &str) -> bool {
        let mut chars = key.chars();
        match chars.next() {
            Some(c) if c == '$' || c == '_' || c.is_ascii_alphabetic() => {}
            _ => return false,
        }

        chars.all(|c| c == '$' || c == '_' || c.is_ascii_alphanumeric())
    }

    fn format_prop_key(key: &str) -> String {
        if is_valid_prop_identifier(key) {
            return key.to_string();
        }

        // Quote invalid identifiers (aria-label, data-*, on:click, etc.)
        let escaped = key.replace('\\', "\\\\").replace('"', "\\\"");
        format!("\"{}\"", escaped)
    }

    let mut static_props: Vec<String> = vec![];
    let mut dynamic_props: Vec<String> = vec![];
    let mut spreads: Vec<String> = vec![];

    for attr in &element.opening_element.attributes {
        match attr {
            JSXAttributeItem::Attribute(attr) => {
                let raw_key = match &attr.name {
                    JSXAttributeName::Identifier(id) => id.name.to_string(),
                    JSXAttributeName::NamespacedName(ns) => {
                        format!("{}:{}", ns.namespace.name, ns.name.name)
                    }
                };
                let key = format_prop_key(&raw_key);

                // Skip component and children props for Dynamic
                if raw_key == "component" || raw_key == "children" {
                    continue;
                }

                // Handle ref prop specially - needs ref forwarding
                if raw_key == "ref" {
                    if let Some(JSXAttributeValue::ExpressionContainer(container)) = &attr.value {
                        if let Some(expr) = container.expression.as_expression() {
                            let expr_str = expr_to_string(expr);
                            // Generate ref forwarding function that handles both callbacks and variables
                            dynamic_props.push(format!(
                                "ref(r$) {{ var _ref$ = {}; typeof _ref$ === \"function\" ? _ref$(r$) : {} = r$; }}",
                                expr_str, expr_str
                            ));
                        }
                    }
                    continue;
                }

                match &attr.value {
                    Some(JSXAttributeValue::StringLiteral(lit)) => {
                        static_props.push(format!("{}: \"{}\"", key, lit.value));
                    }
                    Some(JSXAttributeValue::ExpressionContainer(container)) => {
                        if let Some(expr) = container.expression.as_expression() {
                            let expr_str = expr_to_string(expr);
                            if is_dynamic(expr) {
                                // Dynamic prop - use getter
                                dynamic_props
                                    .push(format!("get {}() {{ return {}; }}", key, expr_str));
                            } else {
                                static_props.push(format!("{}: {}", key, expr_str));
                            }
                        }
                    }
                    None => {
                        static_props.push(format!("{}: true", key));
                    }
                    _ => {}
                }
            }
            JSXAttributeItem::SpreadAttribute(spread) => {
                spreads.push(expr_to_string(&spread.argument));
            }
        }
    }

    // Handle children
    if !element.children.is_empty() {
        let children_expr = get_children_expr_transformed(element, context, transform_child);
        if !children_expr.is_empty() {
            dynamic_props.push(format!("get children() {{ return {}; }}", children_expr));
        }
    }

    // Combine all props
    let all_props = static_props
        .into_iter()
        .chain(dynamic_props)
        .collect::<Vec<_>>()
        .join(", ");

    // Combine props
    if !spreads.is_empty() {
        context.register_helper("mergeProps");
        let spread_list = spreads.join(", ");
        if all_props.is_empty() {
            format!("mergeProps({})", spread_list)
        } else {
            format!("mergeProps({}, {{ {} }})", spread_list, all_props)
        }
    } else if all_props.is_empty() {
        "{}".to_string()
    } else {
        format!("{{ {} }}", all_props)
    }
}

/// Get children as an expression with recursive transformation
fn get_children_expr_transformed<'a, 'b>(
    element: &JSXElement<'a>,
    context: &BlockContext,
    transform_child: ChildTransformer<'a, 'b>,
) -> String {
    let mut children: Vec<String> = vec![];

    for child in &element.children {
        match child {
            JSXChild::Text(text) => {
                let content = common::expression::trim_whitespace(&text.value);
                if !content.is_empty() {
                    children.push(format!(
                        "\"{}\"",
                        common::expression::escape_html(&content, false)
                    ));
                }
            }
            JSXChild::ExpressionContainer(container) => {
                if let Some(expr) = container.expression.as_expression() {
                    children.push(expr_to_string(expr));
                }
            }
            JSXChild::Element(_) | JSXChild::Fragment(_) => {
                // Transform the child JSX element/fragment
                if let Some(result) = transform_child(child) {
                    if let Some(code) = build_child_output(&result, context) {
                        children.push(code);
                    }
                }
            }
            JSXChild::Spread(spread) => {
                children.push(expr_to_string(&spread.expression));
            }
        }
    }

    if children.len() == 1 {
        children.pop().unwrap_or_default()
    } else if children.is_empty() {
        String::new()
    } else {
        format!("[{}]", children.join(", "))
    }
}

// find_prop and get_children_callback moved to common module
