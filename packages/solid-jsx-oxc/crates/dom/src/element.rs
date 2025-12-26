//! Native element transform
//! Handles <div>, <span>, etc. -> template + effects

use oxc_ast::ast::{
    JSXElement, JSXAttribute, JSXAttributeItem, JSXAttributeName,
    JSXAttributeValue,
};

use common::{
    TransformOptions,
    is_svg_element, is_dynamic, expr_to_string, get_attr_name, is_namespaced_attr,
    constants::{ALIASES, DELEGATED_EVENTS, VOID_ELEMENTS},
    expression::{escape_html, to_event_name},
};

use crate::ir::{BlockContext, TransformResult, Declaration, Expr, DynamicBinding};
use crate::transform::TransformInfo;

/// Transform a native HTML/SVG element
pub fn transform_element<'a>(
    element: &JSXElement<'a>,
    tag_name: &str,
    info: &TransformInfo,
    context: &BlockContext,
    options: &TransformOptions<'a>,
) -> TransformResult {
    let is_svg = is_svg_element(tag_name);
    let is_void = VOID_ELEMENTS.contains(tag_name);
    let is_custom_element = tag_name.contains('-');

    let mut result = TransformResult {
        tag_name: Some(tag_name.to_string()),
        is_svg,
        has_custom_element: is_custom_element,
        ..Default::default()
    };

    // Check if this element needs runtime access (dynamic attributes, refs, events)
    let needs_runtime_access = element_needs_runtime_access(element);

    // Generate element ID if needed
    if !info.skip_id && (info.top_level || needs_runtime_access) {
        let elem_id = context.generate_uid("el$");
        result.id = Some(elem_id.clone());

        // If we have a path, we need to walk to this element
        if !info.path.is_empty() {
            if let Some(root_id) = &info.root_id {
                let walk_expr = info.path.iter()
                    .fold(root_id.clone(), |acc, step| format!("{}.{}", acc, step));
                result.declarations.push(Declaration {
                    name: elem_id.clone(),
                    init: walk_expr,
                });
            }
        }
    }

    // Start building template
    result.template = format!("<{}", tag_name);
    result.template_with_closing_tags = result.template.clone();

    // Transform attributes
    transform_attributes(element, &mut result, context, options);

    // Close opening tag
    result.template.push('>');
    result.template_with_closing_tags.push('>');

    // Transform children (if not void element)
    if !is_void {
        // Pass down the root ID and path for children
        let child_info = TransformInfo {
            root_id: if info.top_level {
                result.id.clone()
            } else {
                info.root_id.clone()
            },
            ..info.clone()
        };
        transform_children(element, &mut result, &child_info, context, options);

        // Close tag
        result.template.push_str(&format!("</{}>", tag_name));
        result.template_with_closing_tags.push_str(&format!("</{}>", tag_name));
    }

    result
}

/// Check if an element needs runtime access
fn element_needs_runtime_access(element: &JSXElement) -> bool {
    for attr in &element.opening_element.attributes {
        match attr {
            JSXAttributeItem::Attribute(attr) => {
                // Namespaced attributes like on:click or use:directive always need access
                if is_namespaced_attr(&attr.name) {
                    return true;
                }
                let key = get_attr_name(&attr.name);

                // ref needs access
                if key == "ref" {
                    return true;
                }

                // Event handlers need access
                if key.starts_with("on") && key.len() > 2 {
                    return true;
                }

                // Check if attribute value is dynamic
                if let Some(JSXAttributeValue::ExpressionContainer(container)) = &attr.value {
                    if let Some(expr) = container.expression.as_expression() {
                        if is_dynamic(expr) {
                            return true;
                        }
                    }
                }
            }
            JSXAttributeItem::SpreadAttribute(_) => {
                // Spread attributes always need runtime access
                return true;
            }
        }
    }
    false
}

/// Transform element attributes
fn transform_attributes<'a>(
    element: &JSXElement<'a>,
    result: &mut TransformResult,
    context: &BlockContext,
    options: &TransformOptions<'a>,
) {
    let elem_id = result.id.clone().unwrap_or_else(|| context.generate_uid("el$"));

    for attr in &element.opening_element.attributes {
        match attr {
            JSXAttributeItem::Attribute(attr) => {
                transform_attribute(attr, &elem_id, result, context, options);
            }
            JSXAttributeItem::SpreadAttribute(spread) => {
                // Handle {...props} spread
                context.register_helper("spread");
                let spread_expr = expr_to_string(&spread.argument);
                result.exprs.push(Expr {
                    code: format!(
                        "spread({}, {}, {}, {})",
                        elem_id,
                        spread_expr,
                        result.is_svg,
                        !element.children.is_empty()
                    ),
                });
            }
        }
    }
}

/// Transform a single attribute
fn transform_attribute<'a>(
    attr: &JSXAttribute<'a>,
    elem_id: &str,
    result: &mut TransformResult,
    context: &BlockContext,
    options: &TransformOptions<'a>,
) {
    let key = get_attr_name(&attr.name);

    // Handle different attribute types
    if key == "ref" {
        transform_ref(attr, elem_id, result, context);
        return;
    }

    if key.starts_with("on") {
        transform_event(attr, &key, elem_id, result, context, options);
        return;
    }

    if key.starts_with("use:") {
        transform_directive(attr, &key, elem_id, result, context);
        return;
    }

    // Handle style attribute specially
    if key == "style" {
        transform_style(attr, elem_id, result, context);
        return;
    }

    // Handle innerHTML/textContent
    if key == "innerHTML" || key == "textContent" {
        transform_inner_content(attr, &key, elem_id, result, context);
        return;
    }

    // Regular attribute
    match &attr.value {
        Some(JSXAttributeValue::StringLiteral(lit)) => {
            // Static string attribute - inline in template
            let attr_key = ALIASES.get(key.as_str()).copied().unwrap_or(key.as_str());
            let escaped = escape_html(&lit.value, true);
            result.template.push_str(&format!(" {}=\"{}\"", attr_key, escaped));
        }
        Some(JSXAttributeValue::ExpressionContainer(container)) => {
            // Dynamic attribute - needs effect
            if let Some(expr) = container.expression.as_expression() {
                let expr_str = expr_to_string(expr);
                if is_dynamic(expr) {
                    // Dynamic - wrap in effect
                    result.dynamics.push(DynamicBinding {
                        elem: elem_id.to_string(),
                        key: key.clone(),
                        value: expr_str,
                        is_svg: result.is_svg,
                        is_ce: result.has_custom_element,
                        tag_name: result.tag_name.clone().unwrap_or_default(),
                    });
                } else {
                    // Static expression - we need to evaluate it at build time
                    // For now, treat as dynamic to be safe
                    result.dynamics.push(DynamicBinding {
                        elem: elem_id.to_string(),
                        key: key.clone(),
                        value: expr_str,
                        is_svg: result.is_svg,
                        is_ce: result.has_custom_element,
                        tag_name: result.tag_name.clone().unwrap_or_default(),
                    });
                }
            }
        }
        None => {
            // Boolean attribute (e.g., disabled)
            result.template.push_str(&format!(" {}", key));
        }
        _ => {}
    }
}

/// Transform ref attribute
fn transform_ref<'a>(
    attr: &JSXAttribute<'a>,
    elem_id: &str,
    result: &mut TransformResult,
    context: &BlockContext,
) {
    if let Some(JSXAttributeValue::ExpressionContainer(container)) = &attr.value {
        if let Some(expr) = container.expression.as_expression() {
            let ref_expr = expr_to_string(expr);
            // Check if it's a function or a variable
            if ref_expr.contains("=>") || ref_expr.starts_with("(") {
                // It's a callback: ref={el => myRef = el}
                result.exprs.push(Expr {
                    code: format!("typeof {} === \"function\" ? {}({}) : {} = {}",
                        ref_expr, ref_expr, elem_id, ref_expr, elem_id),
                });
            } else {
                // It's a variable: ref={myRef}
                result.exprs.push(Expr {
                    code: format!("{} = {}", ref_expr, elem_id),
                });
            }
        }
    }
}

/// Transform event handler
fn transform_event<'a>(
    attr: &JSXAttribute<'a>,
    key: &str,
    elem_id: &str,
    result: &mut TransformResult,
    context: &BlockContext,
    options: &TransformOptions<'a>,
) {
    let event_name = to_event_name(key);

    // Get the handler expression
    let handler = if let Some(JSXAttributeValue::ExpressionContainer(container)) = &attr.value {
        container.expression.as_expression()
            .map(|e| expr_to_string(e))
            .unwrap_or_else(|| "undefined".to_string())
    } else {
        "undefined".to_string()
    };

    // Check if this event should be delegated
    let should_delegate = options.delegate_events
        && (DELEGATED_EVENTS.contains(event_name.as_str())
            || options.delegated_events.contains(&event_name.as_str()));

    if should_delegate {
        context.register_delegate(&event_name);
        result.exprs.push(Expr {
            code: format!("{}.$${} = {}", elem_id, event_name, handler),
        });
    } else {
        context.register_helper("addEventListener");
        result.exprs.push(Expr {
            code: format!(
                "addEventListener({}, \"{}\", {}, false)",
                elem_id, event_name, handler
            ),
        });
    }
}

/// Transform use: directive
fn transform_directive<'a>(
    attr: &JSXAttribute<'a>,
    key: &str,
    elem_id: &str,
    result: &mut TransformResult,
    context: &BlockContext,
) {
    context.register_helper("use");
    let directive_name = &key[4..]; // Strip "use:"

    let value = if let Some(JSXAttributeValue::ExpressionContainer(container)) = &attr.value {
        container.expression.as_expression()
            .map(|e| format!("() => {}", expr_to_string(e)))
            .unwrap_or_else(|| "undefined".to_string())
    } else {
        "undefined".to_string()
    };

    result.exprs.push(Expr {
        code: format!(
            "use({}, {}, {})",
            directive_name, elem_id, value
        ),
    });
}

/// Transform style attribute
fn transform_style<'a>(
    attr: &JSXAttribute<'a>,
    elem_id: &str,
    result: &mut TransformResult,
    context: &BlockContext,
) {
    match &attr.value {
        Some(JSXAttributeValue::StringLiteral(lit)) => {
            // Static style string - inline in template
            result.template.push_str(&format!(" style=\"{}\"", escape_html(&lit.value, true)));
        }
        Some(JSXAttributeValue::ExpressionContainer(container)) => {
            if let Some(expr) = container.expression.as_expression() {
                let expr_str = expr_to_string(expr);

                // Check if it's an object expression (static object)
                if let oxc_ast::ast::Expression::ObjectExpression(obj) = expr {
                    // Try to convert to static style string
                    if let Some(style_str) = object_to_style_string(obj) {
                        result.template.push_str(&format!(" style=\"{}\"", style_str));
                        return;
                    }
                }

                // Dynamic style - use style helper
                context.register_helper("style");
                if is_dynamic(expr) {
                    context.register_helper("effect");
                    result.exprs.push(Expr {
                        code: format!("effect(() => style({}, {}))", elem_id, expr_str),
                    });
                } else {
                    result.exprs.push(Expr {
                        code: format!("style({}, {})", elem_id, expr_str),
                    });
                }
            }
        }
        None => {}
        _ => {}
    }
}

/// Try to convert a static object expression to a style string
fn object_to_style_string(obj: &oxc_ast::ast::ObjectExpression) -> Option<String> {
    let mut styles = Vec::new();

    for prop in &obj.properties {
        if let oxc_ast::ast::ObjectPropertyKind::ObjectProperty(prop) = prop {
            // Get key
            let key = match &prop.key {
                oxc_ast::ast::PropertyKey::StaticIdentifier(id) => {
                    // Convert camelCase to kebab-case
                    camel_to_kebab(&id.name)
                }
                oxc_ast::ast::PropertyKey::StringLiteral(lit) => lit.value.to_string(),
                _ => return None, // Dynamic key, can't inline
            };

            // Get value - must be a static literal
            let value = match &prop.value {
                oxc_ast::ast::Expression::StringLiteral(lit) => lit.value.to_string(),
                oxc_ast::ast::Expression::NumericLiteral(num) => {
                    // Add px for numeric values (except certain properties)
                    let num_str = num.value.to_string();
                    if needs_px_suffix(&key) && num.value != 0.0 {
                        format!("{}px", num_str)
                    } else {
                        num_str
                    }
                }
                _ => return None, // Dynamic value, can't inline
            };

            styles.push(format!("{}: {}", key, value));
        } else {
            return None; // Spread or method, can't inline
        }
    }

    Some(styles.join("; "))
}

/// Convert camelCase to kebab-case
fn camel_to_kebab(s: &str) -> String {
    let mut result = String::new();
    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() {
            if i > 0 {
                result.push('-');
            }
            result.push(c.to_lowercase().next().unwrap());
        } else {
            result.push(c);
        }
    }
    result
}

/// Check if a CSS property needs px suffix for numeric values
fn needs_px_suffix(prop: &str) -> bool {
    // Properties that don't need px suffix
    let unitless = [
        "animation-iteration-count", "border-image-outset", "border-image-slice",
        "border-image-width", "box-flex", "box-flex-group", "box-ordinal-group",
        "column-count", "columns", "flex", "flex-grow", "flex-positive",
        "flex-shrink", "flex-negative", "flex-order", "grid-row", "grid-row-end",
        "grid-row-span", "grid-row-start", "grid-column", "grid-column-end",
        "grid-column-span", "grid-column-start", "font-weight", "line-clamp",
        "line-height", "opacity", "order", "orphans", "tab-size", "widows",
        "z-index", "zoom", "fill-opacity", "flood-opacity", "stop-opacity",
        "stroke-dasharray", "stroke-dashoffset", "stroke-miterlimit",
        "stroke-opacity", "stroke-width",
    ];
    !unitless.contains(&prop)
}

/// Transform innerHTML/textContent
fn transform_inner_content<'a>(
    attr: &JSXAttribute<'a>,
    key: &str,
    elem_id: &str,
    result: &mut TransformResult,
    context: &BlockContext,
) {
    if let Some(JSXAttributeValue::ExpressionContainer(container)) = &attr.value {
        if let Some(expr) = container.expression.as_expression() {
            let expr_str = expr_to_string(expr);

            if is_dynamic(expr) {
                context.register_helper("effect");
                result.exprs.push(Expr {
                    code: format!("effect(() => {}.{} = {})", elem_id, key, expr_str),
                });
            } else {
                result.exprs.push(Expr {
                    code: format!("{}.{} = {}", elem_id, key, expr_str),
                });
            }
        }
    } else if let Some(JSXAttributeValue::StringLiteral(lit)) = &attr.value {
        // Static string - but we still need to set it at runtime for innerHTML
        if key == "innerHTML" {
            result.exprs.push(Expr {
                code: format!("{}.innerHTML = \"{}\"", elem_id, escape_html(&lit.value, false)),
            });
        } else {
            // textContent can be inlined in template
            // But the element should have no children then
        }
    }
}

/// Transform element children
fn transform_children<'a>(
    element: &JSXElement<'a>,
    result: &mut TransformResult,
    info: &TransformInfo,
    context: &BlockContext,
    options: &TransformOptions<'a>,
) {
    let mut is_first_element = true;

    for child in &element.children {
        match child {
            oxc_ast::ast::JSXChild::Text(text) => {
                let content = common::expression::trim_whitespace(&text.value);
                if !content.is_empty() {
                    result.template.push_str(&escape_html(&content, false));
                    // Text nodes count as firstChild but we track element children separately
                }
            }
            oxc_ast::ast::JSXChild::Element(child_elem) => {
                // Build the path for this child element
                let mut child_path = info.path.clone();
                if is_first_element {
                    child_path.push("firstChild".to_string());
                } else {
                    child_path.push("nextSibling".to_string());
                }
                is_first_element = false;

                let child_info = TransformInfo {
                    top_level: false,
                    path: child_path,
                    root_id: info.root_id.clone(),
                    ..info.clone()
                };

                // Recursively transform child elements
                let child_tag = common::get_tag_name(child_elem);
                let child_result = transform_element(
                    child_elem,
                    &child_tag,
                    &child_info,
                    context,
                    options,
                );
                result.template.push_str(&child_result.template);
                result.declarations.extend(child_result.declarations);
                result.exprs.extend(child_result.exprs);
                result.dynamics.extend(child_result.dynamics);
            }
            oxc_ast::ast::JSXChild::ExpressionContainer(container) => {
                // Dynamic child - needs insert
                if let Some(expr) = container.expression.as_expression() {
                    context.register_helper("insert");
                    let child_expr = expr_to_string(expr);
                    if let Some(id) = &result.id {
                        // Check if it's a reactive expression
                        if is_dynamic(expr) {
                            result.exprs.push(Expr {
                                code: format!("insert({}, () => {})", id, child_expr),
                            });
                        } else {
                            result.exprs.push(Expr {
                                code: format!("insert({}, {})", id, child_expr),
                            });
                        }
                    }
                }
            }
            _ => {}
        }
    }
}
