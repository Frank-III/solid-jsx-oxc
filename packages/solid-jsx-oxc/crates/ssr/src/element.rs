//! SSR element transform
//!
//! Transforms native HTML elements into SSR template strings.
//! Unlike DOM, we don't create DOM nodes - we build strings.

use oxc_ast::ast::{
    Argument, ArrayExpressionElement, Expression, JSXAttribute, JSXAttributeItem, JSXAttributeName,
    JSXAttributeValue, JSXElement, PropertyKey, PropertyKind,
};
use oxc_span::SPAN;

use common::{
    constants::{ALIASES, CHILD_PROPERTIES, PROPERTIES, VOID_ELEMENTS},
    expression::escape_html,
    get_attr_name, is_svg_element, TransformOptions,
};

use crate::ir::{SSRContext, SSRResult};

/// Transform a native HTML/SVG element for SSR
pub fn transform_element<'a>(
    element: &JSXElement<'a>,
    tag_name: &str,
    context: &SSRContext<'a>,
    options: &TransformOptions<'a>,
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

    // Add hydration key if needed
    if context.hydratable && options.hydratable {
        context.register_helper("ssrHydrationKey");
        let callee = ast.expression_identifier(SPAN, "ssrHydrationKey");
        let expr = ast.expression_call(
            SPAN,
            callee,
            None::<oxc_ast::ast::TSTypeParameterInstantiation<'a>>,
            ast.vec(),
            false,
        );
        result.push_dynamic(expr, false, true);
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

    // Build props object - merge spreads with regular attributes
    let is_svg = is_svg_element(tag_name);
    let mut props = ast.vec();

    for attr in &element.opening_element.attributes {
        match attr {
            JSXAttributeItem::SpreadAttribute(spread) => {
                props.push(ast.object_property_kind_spread_property(
                    span,
                    context.clone_expr(&spread.argument),
                ));
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

                match &attr.value {
                    Some(JSXAttributeValue::StringLiteral(lit)) => {
                        let key = PropertyKey::StringLiteral(ast.alloc_string_literal(
                            span,
                            ast.allocator.alloc_str(&attr_name),
                            None,
                        ));
                        let value = ast.expression_string_literal(
                            span,
                            ast.allocator.alloc_str(&escape_html(&lit.value, true)),
                            None,
                        );
                        props.push(ast.object_property_kind_object_property(
                            span,
                            PropertyKind::Init,
                            key,
                            value,
                            false,
                            false,
                            false,
                        ));
                    }
                    Some(JSXAttributeValue::ExpressionContainer(container)) => {
                        if let Some(expr) = container.expression.as_expression() {
                            let key = PropertyKey::StringLiteral(ast.alloc_string_literal(
                                span,
                                ast.allocator.alloc_str(&attr_name),
                                None,
                            ));
                            props.push(ast.object_property_kind_object_property(
                                span,
                                PropertyKind::Init,
                                key,
                                context.clone_expr(expr),
                                false,
                                false,
                                false,
                            ));
                        }
                    }
                    None => {
                        let key = PropertyKey::StringLiteral(ast.alloc_string_literal(
                            span,
                            ast.allocator.alloc_str(&attr_name),
                            None,
                        ));
                        let value = ast.expression_boolean_literal(span, true);
                        props.push(ast.object_property_kind_object_property(
                            span,
                            PropertyKind::Init,
                            key,
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

    let props_expr = ast.expression_object(span, props);

    // Build children
    let children_expr = if element.children.is_empty() {
        ast.expression_null_literal(span)
    } else {
        let mut children: Vec<Expression<'a>> = Vec::new();
        for child in &element.children {
            match child {
                oxc_ast::ast::JSXChild::Text(text) => {
                    let content = common::expression::trim_whitespace(&text.value);
                    if !content.is_empty() {
                        children.push(ast.expression_string_literal(
                            span,
                            ast.allocator.alloc_str(&escape_html(&content, false)),
                            None,
                        ));
                    }
                }
                oxc_ast::ast::JSXChild::ExpressionContainer(container) => {
                    if let Some(expr) = container.expression.as_expression() {
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
                    }
                }
                oxc_ast::ast::JSXChild::Element(child_elem) => {
                    // Recursively transform child element - check if component or native
                    let child_tag = common::get_tag_name(child_elem);
                    let child_result = if common::is_component(&child_tag) {
                        // Component - use component transformer
                        let child_transformer =
                            |child: &oxc_ast::ast::JSXChild<'a>| -> Option<SSRResult<'a>> {
                                match child {
                                    oxc_ast::ast::JSXChild::Element(el) => {
                                        let tag = common::get_tag_name(el);
                                        Some(if common::is_component(&tag) {
                                            // For deeply nested components, use simple fallback
                                            context.register_helper("createComponent");
                                            context.register_helper("escape");

                                            let mut r = SSRResult::new();
                                            r.span = el.span;
                                            let callee =
                                                ast.expression_identifier(span, "createComponent");
                                            let mut args = ast.vec();
                                            let tag_expr = ast.expression_identifier(
                                                span,
                                                ast.allocator.alloc_str(&tag),
                                            );
                                            args.push(Argument::from(tag_expr));
                                            args.push(Argument::from(
                                                ast.expression_object(span, ast.vec()),
                                            ));
                                            let call = ast.expression_call(
                                                span,
                                                callee,
                                                None::<
                                                    oxc_ast::ast::TSTypeParameterInstantiation<'a>,
                                                >,
                                                args,
                                                false,
                                            );
                                            r.push_dynamic(call, false, false);
                                            r
                                        } else {
                                            transform_element(el, &tag, context, options)
                                        })
                                    }
                                    _ => None,
                                }
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

                    children.push(child_result.to_ssr_expression(ast, context.hydratable));
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
    let callee = ast.expression_identifier(span, "ssrElement");
    let mut args = ast.vec();
    args.push(Argument::from(ast.expression_string_literal(
        span,
        ast.allocator.alloc_str(tag_name),
        None,
    )));
    args.push(Argument::from(props_expr));
    args.push(Argument::from(children_expr));
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

/// Transform element attributes for SSR
fn transform_attributes<'a>(
    element: &JSXElement<'a>,
    result: &mut SSRResult<'a>,
    context: &SSRContext<'a>,
    options: &TransformOptions<'a>,
) {
    let tag_name = result.tag_name.as_deref().unwrap_or("");
    let is_svg = is_svg_element(tag_name);

    for attr in &element.opening_element.attributes {
        if let JSXAttributeItem::Attribute(attr) = attr {
            transform_attribute(attr, result, context, options, is_svg);
        }
    }
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
                    result.push_dynamic(
                        ast.expression_call(
                            SPAN,
                            callee,
                            None::<oxc_ast::ast::TSTypeParameterInstantiation<'a>>,
                            args,
                            false,
                        ),
                        false,
                        true,
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
                    result.push_dynamic(
                        ast.expression_call(
                            SPAN,
                            callee,
                            None::<oxc_ast::ast::TSTypeParameterInstantiation<'a>>,
                            args,
                            false,
                        ),
                        false,
                        true,
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
                    result.push_dynamic(
                        ast.expression_call(
                            SPAN,
                            callee,
                            None::<oxc_ast::ast::TSTypeParameterInstantiation<'a>>,
                            args,
                            false,
                        ),
                        false,
                        true,
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
    process_jsx_children(&element.children, result, skip_escape, context, options);
}

/// Process a list of JSX children, appending to the result.
/// This is extracted as a helper to enable recursive processing of fragment children.
fn process_jsx_children<'a>(
    children: &oxc_allocator::Vec<'a, oxc_ast::ast::JSXChild<'a>>,
    result: &mut SSRResult<'a>,
    skip_escape: bool,
    context: &SSRContext<'a>,
    options: &TransformOptions<'a>,
) {
    let ast = context.ast();
    for child in children {
        match child {
            oxc_ast::ast::JSXChild::Text(text) => {
                let content = common::expression::trim_whitespace(&text.value);
                if !content.is_empty() {
                    if skip_escape {
                        result.push_static(&content);
                    } else {
                        result.push_static(&escape_html(&content, false));
                    }
                }
            }

            oxc_ast::ast::JSXChild::Element(child_elem) => {
                let child_tag = common::get_tag_name(child_elem);
                let child_result = if common::is_component(&child_tag) {
                    // Create a child transformer for nested components
                    let child_transformer =
                        |child: &oxc_ast::ast::JSXChild<'a>| -> Option<SSRResult<'a>> {
                            match child {
                                oxc_ast::ast::JSXChild::Element(el) => {
                                    let tag = common::get_tag_name(el);
                                    Some(if common::is_component(&tag) {
                                        // For deeply nested components, use simple fallback
                                        context.register_helper("createComponent");
                                        context.register_helper("escape");
                                        let mut r = SSRResult::new();
                                        r.span = el.span;
                                        let callee =
                                            ast.expression_identifier(SPAN, "createComponent");
                                        let mut args = ast.vec();
                                        let tag_expr = ast.expression_identifier(
                                            SPAN,
                                            ast.allocator.alloc_str(&tag),
                                        );
                                        args.push(Argument::from(tag_expr));
                                        args.push(Argument::from(
                                            ast.expression_object(SPAN, ast.vec()),
                                        ));
                                        let call = ast.expression_call(
                                            SPAN,
                                            callee,
                                            None::<oxc_ast::ast::TSTypeParameterInstantiation<'a>>,
                                            args,
                                            false,
                                        );
                                        r.push_dynamic(call, false, false);
                                        r
                                    } else {
                                        transform_element(el, &tag, context, options)
                                    })
                                }
                                _ => None,
                            }
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
                result.merge(child_result);
            }

            oxc_ast::ast::JSXChild::ExpressionContainer(container) => {
                if let Some(expr) = container.expression.as_expression() {
                    let expr = context.clone_expr(expr);

                    if skip_escape {
                        // Inside script/style - don't escape
                        result.push_dynamic(expr, false, true);
                    } else {
                        // Normal content - escape
                        context.register_helper("escape");
                        result.push_dynamic(expr, false, false);
                    }
                }
            }

            oxc_ast::ast::JSXChild::Fragment(fragment) => {
                // Recursively process fragment children with same escape settings
                process_jsx_children(&fragment.children, result, skip_escape, context, options);
            }

            _ => {}
        }
    }
}
