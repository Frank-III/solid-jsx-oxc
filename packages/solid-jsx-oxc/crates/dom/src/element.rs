//! Native element transform
//! Handles <div>, <span>, etc. -> template + effects

use oxc_ast::ast::{JSXAttribute, JSXAttributeItem, JSXAttributeValue, JSXElement};

use common::{
    constants::{ALIASES, DELEGATED_EVENTS, VOID_ELEMENTS},
    expr_to_string,
    expression::{escape_html, to_event_name},
    get_attr_name, is_component, is_dynamic, is_namespaced_attr, is_svg_element, TransformOptions,
};

use crate::ir::{
    BlockContext, ChildTransformer, Declaration, DynamicBinding, Expr, TransformResult,
};
use crate::transform::TransformInfo;

/// Transform a native HTML/SVG element
pub fn transform_element<'a, 'b>(
    element: &JSXElement<'a>,
    tag_name: &str,
    info: &TransformInfo,
    context: &BlockContext,
    options: &TransformOptions<'a>,
    transform_child: ChildTransformer<'a, 'b>,
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
                let walk_expr = info
                    .path
                    .iter()
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
        // If this element has an ID, it becomes the new root for children
        // and children's paths reset to be relative to this element
        let child_info = TransformInfo {
            root_id: result.id.clone().or_else(|| info.root_id.clone()),
            path: if result.id.is_some() {
                vec![]
            } else {
                info.path.clone()
            },
            top_level: false,
            ..info.clone()
        };
        transform_children(
            element,
            &mut result,
            &child_info,
            context,
            options,
            transform_child,
        );

        // Close tag
        result.template.push_str(&format!("</{}>", tag_name));
        result
            .template_with_closing_tags
            .push_str(&format!("</{}>", tag_name));
    }

    result
}

/// Check if an element needs runtime access
fn element_needs_runtime_access(element: &JSXElement) -> bool {
    // Check attributes
    for attr in &element.opening_element.attributes {
        match attr {
            JSXAttributeItem::Attribute(attr) => {
                // Namespaced attributes like on:click or use:directive always need access
                if is_namespaced_attr(&attr.name) {
                    return true;
                }
                let key = get_attr_name(&attr.name);

                // ref and inner content setters need access
                if key == "ref" || key == "innerHTML" || key == "textContent" || key == "innerText"
                {
                    return true;
                }

                // Event handlers need access
                if key.starts_with("on") && key.len() > 2 {
                    return true;
                }

                // Any expression container needs runtime access (we may need to run setters/helpers).
                // This keeps id generation consistent with the rest of the transform.
                if matches!(&attr.value, Some(JSXAttributeValue::ExpressionContainer(_))) {
                    return true;
                }
            }
            JSXAttributeItem::SpreadAttribute(_) => {
                // Spread attributes always need runtime access
                return true;
            }
        }
    }

    // Check children for components or dynamic expressions
    // If any child is a component, we need an ID for insert() calls
    fn children_need_runtime_access<'a>(children: &[oxc_ast::ast::JSXChild<'a>]) -> bool {
        for child in children {
            match child {
                oxc_ast::ast::JSXChild::Element(child_elem) => {
                    let child_tag = common::get_tag_name(child_elem);
                    if is_component(&child_tag) {
                        return true;
                    }
                }
                oxc_ast::ast::JSXChild::ExpressionContainer(_) => {
                    return true;
                }
                oxc_ast::ast::JSXChild::Fragment(fragment) => {
                    if children_need_runtime_access(&fragment.children) {
                        return true;
                    }
                }
                _ => {}
            }
        }
        false
    }

    if children_need_runtime_access(&element.children) {
        return true;
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
    let elem_id = result.id.clone();

    for attr in &element.opening_element.attributes {
        match attr {
            JSXAttributeItem::Attribute(attr) => {
                transform_attribute(attr, elem_id.as_deref(), result, context, options);
            }
            JSXAttributeItem::SpreadAttribute(spread) => {
                // Handle {...props} spread
                let elem_id = elem_id
                    .as_deref()
                    .expect("Spread attributes require an element id");
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
    elem_id: Option<&str>,
    result: &mut TransformResult,
    context: &BlockContext,
    options: &TransformOptions<'a>,
) {
    let key = get_attr_name(&attr.name);

    // Handle different attribute types
    if key == "ref" {
        let elem_id = elem_id.expect("ref requires an element id");
        transform_ref(attr, elem_id, result, context);
        return;
    }

    if key.starts_with("on") {
        let elem_id = elem_id.expect("event handlers require an element id");
        transform_event(attr, &key, elem_id, result, context, options);
        return;
    }

    if key.starts_with("use:") {
        let elem_id = elem_id.expect("directives require an element id");
        transform_directive(attr, &key, elem_id, result, context);
        return;
    }

    // Handle prop: prefix - direct DOM property assignment
    if key.starts_with("prop:") {
        let elem_id = elem_id.expect("prop: requires an element id");
        transform_prop(attr, &key, elem_id, result, context);
        return;
    }

    // Handle attr: prefix - force attribute mode
    if key.starts_with("attr:") {
        let elem_id = elem_id.expect("attr: requires an element id");
        transform_attr(attr, &key, elem_id, result, context);
        return;
    }

    // Handle style attribute specially
    if key == "style" {
        transform_style(attr, elem_id, result, context);
        return;
    }

    // Handle innerHTML/textContent
    if key == "innerHTML" || key == "textContent" {
        let elem_id = elem_id.expect("inner content requires an element id");
        transform_inner_content(attr, &key, elem_id, result, context);
        return;
    }

    // Regular attribute
    match &attr.value {
        Some(JSXAttributeValue::StringLiteral(lit)) => {
            // Static string attribute - inline in template
            let attr_key = ALIASES.get(key.as_str()).copied().unwrap_or(key.as_str());
            let escaped = escape_html(&lit.value, true);
            result
                .template
                .push_str(&format!(" {}=\"{}\"", attr_key, escaped));
        }
        Some(JSXAttributeValue::ExpressionContainer(container)) => {
            // Dynamic attribute - needs effect
            if let Some(expr) = container.expression.as_expression() {
                let expr_str = expr_to_string(expr);
                if is_dynamic(expr) {
                    // Dynamic - wrap in effect
                    let elem_id = elem_id.expect("dynamic attributes require an element id");
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
                    let elem_id = elem_id.expect("expression attributes require an element id");
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
    _context: &BlockContext,
) {
    if let Some(JSXAttributeValue::ExpressionContainer(container)) = &attr.value {
        if let Some(expr) = container.expression.as_expression() {
            let ref_expr = expr_to_string(expr);
            // Check if it's a function expression (arrow function or function expression)
            if ref_expr.contains("=>") || ref_expr.starts_with("function") {
                // It's an inline callback: ref={el => myRef = el}
                // Just invoke it with the element
                result.exprs.push(Expr {
                    code: format!("({})({})", ref_expr, elem_id),
                });
            } else {
                // It's a variable reference: ref={myRef}
                // Could be a signal setter or plain variable - check at runtime
                result.exprs.push(Expr {
                    code: format!(
                        "typeof {} === \"function\" ? {}({}) : {} = {}",
                        ref_expr, ref_expr, elem_id, ref_expr, elem_id
                    ),
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
    // Check for capture mode (onClickCapture -> click with capture=true)
    let is_capture = key.ends_with("Capture");
    let base_key = if is_capture {
        &key[..key.len() - 7] // Remove "Capture" suffix
    } else {
        key
    };

    let event_name = to_event_name(base_key);

    // Get the handler expression
    let handler = if let Some(JSXAttributeValue::ExpressionContainer(container)) = &attr.value {
        container
            .expression
            .as_expression()
            .map(|e| expr_to_string(e))
            .unwrap_or_else(|| "undefined".to_string())
    } else {
        "undefined".to_string()
    };

    // on: prefix forces non-delegation (direct addEventListener)
    let force_no_delegate = key.starts_with("on:");

    // Capture events cannot be delegated
    // Check if this event should be delegated
    let should_delegate = !force_no_delegate
        && !is_capture
        && options.delegate_events
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
                "addEventListener({}, \"{}\", {}, {})",
                elem_id, event_name, handler, is_capture
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
        container
            .expression
            .as_expression()
            .map(|e| format!("() => {}", expr_to_string(e)))
            .unwrap_or_else(|| "undefined".to_string())
    } else {
        "undefined".to_string()
    };

    result.exprs.push(Expr {
        code: format!("use({}, {}, {})", directive_name, elem_id, value),
    });
}

/// Transform prop: prefix (direct DOM property assignment)
fn transform_prop<'a>(
    attr: &JSXAttribute<'a>,
    key: &str,
    elem_id: &str,
    result: &mut TransformResult,
    context: &BlockContext,
) {
    let prop_name = &key[5..]; // Strip "prop:"

    if let Some(JSXAttributeValue::ExpressionContainer(container)) = &attr.value {
        if let Some(expr) = container.expression.as_expression() {
            let expr_str = expr_to_string(expr);
            if is_dynamic(expr) {
                context.register_helper("effect");
                result.exprs.push(Expr {
                    code: format!("effect(() => {}.{} = {})", elem_id, prop_name, expr_str),
                });
            } else {
                result.exprs.push(Expr {
                    code: format!("{}.{} = {}", elem_id, prop_name, expr_str),
                });
            }
        }
    }
}

/// Transform attr: prefix (force attribute mode via setAttribute)
fn transform_attr<'a>(
    attr: &JSXAttribute<'a>,
    key: &str,
    elem_id: &str,
    result: &mut TransformResult,
    context: &BlockContext,
) {
    let attr_name = &key[5..]; // Strip "attr:"

    if let Some(JSXAttributeValue::ExpressionContainer(container)) = &attr.value {
        if let Some(expr) = container.expression.as_expression() {
            let expr_str = expr_to_string(expr);
            context.register_helper("effect");
            context.register_helper("setAttribute");
            result.exprs.push(Expr {
                code: format!(
                    "effect(() => {}.setAttribute(\"{}\", {}))",
                    elem_id, attr_name, expr_str
                ),
            });
        }
    } else if let Some(JSXAttributeValue::StringLiteral(lit)) = &attr.value {
        // Static value - inline in template
        let escaped = escape_html(&lit.value, true);
        result
            .template
            .push_str(&format!(" {}=\"{}\"", attr_name, escaped));
    }
}

/// Transform style attribute
fn transform_style<'a>(
    attr: &JSXAttribute<'a>,
    elem_id: Option<&str>,
    result: &mut TransformResult,
    context: &BlockContext,
) {
    match &attr.value {
        Some(JSXAttributeValue::StringLiteral(lit)) => {
            // Static style string - inline in template
            result
                .template
                .push_str(&format!(" style=\"{}\"", escape_html(&lit.value, true)));
        }
        Some(JSXAttributeValue::ExpressionContainer(container)) => {
            if let Some(expr) = container.expression.as_expression() {
                let expr_str = expr_to_string(expr);

                // Check if it's an object expression (static object)
                if let oxc_ast::ast::Expression::ObjectExpression(obj) = expr {
                    // Try to convert to static style string
                    if let Some(style_str) = object_to_style_string(obj) {
                        result
                            .template
                            .push_str(&format!(" style=\"{}\"", style_str));
                        return;
                    }
                }

                // Dynamic style - use style helper
                let elem_id = elem_id.expect("style helper requires an element id");
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
        "animation-iteration-count",
        "border-image-outset",
        "border-image-slice",
        "border-image-width",
        "box-flex",
        "box-flex-group",
        "box-ordinal-group",
        "column-count",
        "columns",
        "flex",
        "flex-grow",
        "flex-positive",
        "flex-shrink",
        "flex-negative",
        "flex-order",
        "grid-row",
        "grid-row-end",
        "grid-row-span",
        "grid-row-start",
        "grid-column",
        "grid-column-end",
        "grid-column-span",
        "grid-column-start",
        "font-weight",
        "line-clamp",
        "line-height",
        "opacity",
        "order",
        "orphans",
        "tab-size",
        "widows",
        "z-index",
        "zoom",
        "fill-opacity",
        "flood-opacity",
        "stop-opacity",
        "stroke-dasharray",
        "stroke-dashoffset",
        "stroke-miterlimit",
        "stroke-opacity",
        "stroke-width",
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
                code: format!(
                    "{}.innerHTML = \"{}\"",
                    elem_id,
                    escape_html(&lit.value, false)
                ),
            });
        } else {
            // textContent can be inlined in template
            // But the element should have no children then
        }
    }
}

/// Transform element children
fn transform_children<'a, 'b>(
    element: &JSXElement<'a>,
    result: &mut TransformResult,
    info: &TransformInfo,
    context: &BlockContext,
    options: &TransformOptions<'a>,
    transform_child: ChildTransformer<'a, 'b>,
) {
    fn child_path(base: &[String], node_index: usize) -> Vec<String> {
        let mut path = base.to_vec();
        path.push("firstChild".to_string());
        for _ in 0..node_index {
            path.push("nextSibling".to_string());
        }
        path
    }

    fn child_accessor(parent_id: &str, node_index: usize) -> String {
        let mut access = format!("{}.firstChild", parent_id);
        for _ in 0..node_index {
            access.push_str(".nextSibling");
        }
        access
    }

    /// Check if children list is a single dynamic expression (no markers needed)
    fn is_single_dynamic_child(children: &[oxc_ast::ast::JSXChild<'_>]) -> bool {
        let mut expr_count = 0;
        let mut other_content = false;

        for child in children {
            match child {
                oxc_ast::ast::JSXChild::Text(text) => {
                    let content = common::expression::trim_whitespace(&text.value);
                    if !content.is_empty() {
                        other_content = true;
                    }
                }
                oxc_ast::ast::JSXChild::Element(_) => {
                    other_content = true;
                }
                oxc_ast::ast::JSXChild::ExpressionContainer(container) => {
                    if container.expression.as_expression().is_some() {
                        expr_count += 1;
                    }
                }
                oxc_ast::ast::JSXChild::Fragment(fragment) => {
                    // Recurse into fragments
                    if !is_single_dynamic_child(&fragment.children) {
                        other_content = true;
                    } else {
                        expr_count += 1;
                    }
                }
                _ => {}
            }
        }

        expr_count == 1 && !other_content
    }

    fn transform_children_list<'a, 'b>(
        children: &[oxc_ast::ast::JSXChild<'a>],
        result: &mut TransformResult,
        info: &TransformInfo,
        context: &BlockContext,
        options: &TransformOptions<'a>,
        transform_child: ChildTransformer<'a, 'b>,
        node_index: &mut usize,
        last_was_text: &mut bool,
        single_dynamic: bool,
    ) {
        for child in children {
            match child {
                oxc_ast::ast::JSXChild::Text(text) => {
                    let content = common::expression::trim_whitespace(&text.value);
                    if !content.is_empty() {
                        let escaped = escape_html(&content, false);
                        result.template.push_str(&escaped);
                        result.template_with_closing_tags.push_str(&escaped);
                        if !*last_was_text {
                            *node_index += 1;
                            *last_was_text = true;
                        }
                    }
                }
                oxc_ast::ast::JSXChild::Element(child_elem) => {
                    let child_tag = common::get_tag_name(child_elem);

                    if is_component(&child_tag) {
                        *last_was_text = false;
                        if let (Some(parent_id), Some(child_result)) =
                            (result.id.as_deref(), transform_child(child))
                        {
                            if child_result.exprs.is_empty() {
                                continue;
                            }

                            context.register_helper("insert");

                            // Single dynamic child: no marker needed
                            if single_dynamic {
                                result.exprs.push(Expr {
                                    code: format!(
                                        "insert({}, {})",
                                        parent_id, child_result.exprs[0].code
                                    ),
                                });
                            } else {
                                result.template.push_str("<!>");
                                result.template_with_closing_tags.push_str("<!>");

                                let marker_id = context.generate_uid("el$");
                                result.declarations.push(Declaration {
                                    name: marker_id.clone(),
                                    init: child_accessor(parent_id, *node_index),
                                });

                                result.exprs.push(Expr {
                                    code: format!(
                                        "insert({}, {}, {})",
                                        parent_id, child_result.exprs[0].code, marker_id
                                    ),
                                });

                                *node_index += 1;
                            }
                        }
                        continue;
                    }

                    *last_was_text = false;
                    let child_info = TransformInfo {
                        top_level: false,
                        path: child_path(&info.path, *node_index),
                        root_id: info.root_id.clone(),
                        ..info.clone()
                    };

                    let child_result = transform_element(
                        child_elem,
                        &child_tag,
                        &child_info,
                        context,
                        options,
                        transform_child,
                    );

                    result.template.push_str(&child_result.template);
                    if !child_result.template_with_closing_tags.is_empty() {
                        result
                            .template_with_closing_tags
                            .push_str(&child_result.template_with_closing_tags);
                    } else {
                        result
                            .template_with_closing_tags
                            .push_str(&child_result.template);
                    }
                    result.declarations.extend(child_result.declarations);
                    result.exprs.extend(child_result.exprs);
                    result.dynamics.extend(child_result.dynamics);
                    result.has_custom_element |= child_result.has_custom_element;

                    *node_index += 1;
                }
                oxc_ast::ast::JSXChild::ExpressionContainer(container) => {
                    if let (Some(parent_id), Some(expr)) =
                        (result.id.as_deref(), container.expression.as_expression())
                    {
                        *last_was_text = false;
                        context.register_helper("insert");

                        let expr_str = expr_to_string(expr);
                        let insert_value = if is_dynamic(expr) {
                            format!("() => {}", expr_str)
                        } else {
                            expr_str
                        };

                        // Single dynamic child: no marker needed
                        if single_dynamic {
                            result.exprs.push(Expr {
                                code: format!("insert({}, {})", parent_id, insert_value),
                            });
                        } else {
                            result.template.push_str("<!>");
                            result.template_with_closing_tags.push_str("<!>");

                            let marker_id = context.generate_uid("el$");
                            result.declarations.push(Declaration {
                                name: marker_id.clone(),
                                init: child_accessor(parent_id, *node_index),
                            });

                            result.exprs.push(Expr {
                                code: format!(
                                    "insert({}, {}, {})",
                                    parent_id, insert_value, marker_id
                                ),
                            });

                            *node_index += 1;
                        }
                    }
                }
                oxc_ast::ast::JSXChild::Fragment(fragment) => {
                    transform_children_list(
                        &fragment.children,
                        result,
                        info,
                        context,
                        options,
                        transform_child,
                        node_index,
                        last_was_text,
                        single_dynamic,
                    );
                }
                _ => {}
            }
        }
    }

    let mut node_index = 0usize;
    let mut last_was_text = false;
    let single_dynamic = is_single_dynamic_child(&element.children);
    transform_children_list(
        &element.children,
        result,
        info,
        context,
        options,
        transform_child,
        &mut node_index,
        &mut last_was_text,
        single_dynamic,
    );
}
