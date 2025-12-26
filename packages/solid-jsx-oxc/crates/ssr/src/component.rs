//! SSR component transform
//!
//! Components in SSR are rendered the same way as DOM - using createComponent.
//! The component itself decides whether to render for server or client.

use oxc_ast::ast::{
    JSXElement, JSXAttributeItem, JSXAttributeName,
    JSXAttributeValue, JSXChild,
};

use common::{TransformOptions, is_built_in, is_dynamic, expr_to_string, get_children_callback, find_prop_value};

use crate::ir::{SSRContext, SSRResult, SSRChildTransformer};

// find_prop_value and get_children_callback moved to common module

/// Get children as SSR expression with recursive transformation
fn get_children_ssr<'a, 'b>(
    element: &JSXElement<'a>,
    transform_child: SSRChildTransformer<'a, 'b>,
) -> String {
    let mut children: Vec<String> = vec![];

    for child in &element.children {
        match child {
            JSXChild::Text(text) => {
                let content = common::expression::trim_whitespace(&text.value);
                if !content.is_empty() {
                    children.push(format!("\"{}\"", common::expression::escape_html(&content, false)));
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
                    children.push(result.to_ssr_call());
                }
            }
            JSXChild::Spread(spread) => {
                children.push(expr_to_string(&spread.expression));
            }
        }
    }

    if children.len() == 1 {
        format!("() => {}", children.pop().unwrap_or_default())
    } else if children.is_empty() {
        "undefined".to_string()
    } else {
        format!("() => [{}]", children.join(", "))
    }
}

/// Transform a component for SSR
pub fn transform_component<'a, 'b>(
    element: &JSXElement<'a>,
    tag_name: &str,
    context: &SSRContext,
    options: &TransformOptions<'a>,
    transform_child: SSRChildTransformer<'a, 'b>,
) -> SSRResult {
    let mut result = SSRResult::new();

    // Check if this is a built-in (For, Show, etc.)
    if is_built_in(tag_name) {
        return transform_builtin(element, tag_name, context, transform_child);
    }

    context.register_helper("createComponent");
    context.register_helper("escape");

    // Build props
    let props = build_props(element, context, options, transform_child);

    // Generate createComponent call - will be escaped by parent
    result.push_dynamic(
        format!("createComponent({}, {})", tag_name, props),
        false,
        false, // Components return escaped content
    );

    result
}

/// Transform built-in control flow components for SSR
fn transform_builtin<'a, 'b>(
    element: &JSXElement<'a>,
    tag_name: &str,
    context: &SSRContext,
    transform_child: SSRChildTransformer<'a, 'b>,
) -> SSRResult {
    let mut result = SSRResult::new();

    context.register_helper("createComponent");
    context.register_helper("escape");

    match tag_name {
        "For" => {
            context.register_helper("For");
            let each = find_prop_value(element, "each").unwrap_or("[]".to_string());
            let children = get_children_callback(element);
            result.push_dynamic(
                format!("createComponent(For, {{ each: {}, children: {} }})", each, children),
                false,
                false,
            );
        }

        "Show" => {
            context.register_helper("Show");
            let when = find_prop_value(element, "when").unwrap_or("false".to_string());
            let fallback = find_prop_value(element, "fallback").unwrap_or("undefined".to_string());
            let children = get_children_ssr(element, transform_child);
            result.push_dynamic(
                format!("createComponent(Show, {{ when: {}, fallback: {}, children: {} }})", when, fallback, children),
                false,
                false,
            );
        }

        "Switch" => {
            context.register_helper("Switch");
            let children = get_children_ssr(element, transform_child);
            result.push_dynamic(
                format!("createComponent(Switch, {{ children: {} }})", children),
                false,
                false,
            );
        }

        "Match" => {
            context.register_helper("Match");
            let when = find_prop_value(element, "when").unwrap_or("false".to_string());
            let children = get_children_ssr(element, transform_child);
            result.push_dynamic(
                format!("createComponent(Match, {{ when: {}, children: {} }})", when, children),
                false,
                false,
            );
        }

        "Index" => {
            context.register_helper("Index");
            let each = find_prop_value(element, "each").unwrap_or("[]".to_string());
            let children = get_children_callback(element);
            result.push_dynamic(
                format!("createComponent(Index, {{ each: {}, children: {} }})", each, children),
                false,
                false,
            );
        }

        "Suspense" => {
            context.register_helper("Suspense");
            let fallback = find_prop_value(element, "fallback").unwrap_or("undefined".to_string());
            let children = get_children_ssr(element, transform_child);
            result.push_dynamic(
                format!("createComponent(Suspense, {{ fallback: {}, children: {} }})", fallback, children),
                false,
                false,
            );
        }

        "Portal" => {
            // Portal in SSR just renders children (no mount target on server)
            context.register_helper("Portal");
            let children = get_children_ssr(element, transform_child);
            result.push_dynamic(
                format!("createComponent(Portal, {{ children: {} }})", children),
                false,
                false,
            );
        }

        "Dynamic" => {
            context.register_helper("Dynamic");
            let component = find_prop_value(element, "component").unwrap_or("undefined".to_string());
            result.push_dynamic(
                format!("createComponent(Dynamic, {{ component: {} }})", component),
                false,
                false,
            );
        }

        "ErrorBoundary" => {
            context.register_helper("ErrorBoundary");
            let fallback = find_prop_value(element, "fallback").unwrap_or("undefined".to_string());
            let children = get_children_ssr(element, transform_child);
            result.push_dynamic(
                format!("createComponent(ErrorBoundary, {{ fallback: {}, children: {} }})", fallback, children),
                false,
                false,
            );
        }

        "NoHydration" => {
            // Special SSR component - renders children without hydration markers
            context.register_helper("NoHydration");
            let children = get_children_ssr(element, transform_child);
            result.push_dynamic(
                format!("createComponent(NoHydration, {{ children: {} }})", children),
                false,
                true, // Don't escape - it handles its own output
            );
        }

        _ => {
            // Unknown built-in, treat as regular component
            result.push_dynamic(
                format!("createComponent({}, {{}})", tag_name),
                false,
                false,
            );
        }
    }

    result
}

/// Build props object for a component
fn build_props<'a, 'b>(
    element: &JSXElement<'a>,
    context: &SSRContext,
    _options: &TransformOptions<'a>,
    transform_child: SSRChildTransformer<'a, 'b>,
) -> String {
    let mut static_props: Vec<String> = vec![];
    let mut dynamic_props: Vec<String> = vec![];
    let mut spreads: Vec<String> = vec![];

    for attr in &element.opening_element.attributes {
        match attr {
            JSXAttributeItem::Attribute(attr) => {
                let key = match &attr.name {
                    JSXAttributeName::Identifier(id) => id.name.to_string(),
                    JSXAttributeName::NamespacedName(ns) => {
                        format!("{}:{}", ns.namespace.name, ns.name.name)
                    }
                };

                // Skip event handlers and refs in SSR
                if key.starts_with("on") || key == "ref" || key.starts_with("use:") {
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
                                dynamic_props.push(format!(
                                    "get {}() {{ return {}; }}",
                                    key, expr_str
                                ));
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
        let children = get_children_ssr(element, transform_child);
        dynamic_props.push(format!("get children() {{ return {}; }}", children));
    }

    // Combine all props
    let all_props = static_props.into_iter()
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
