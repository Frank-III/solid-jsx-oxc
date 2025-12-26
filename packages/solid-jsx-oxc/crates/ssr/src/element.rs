//! SSR element transform
//!
//! Transforms native HTML elements into SSR template strings.
//! Unlike DOM, we don't create DOM nodes - we build strings.

use oxc_ast::ast::{
    JSXElement, JSXAttribute, JSXAttributeItem, JSXAttributeName,
    JSXAttributeValue,
};

use common::{
    TransformOptions,
    is_svg_element, expr_to_string, get_attr_name,
    constants::{PROPERTIES, CHILD_PROPERTIES, ALIASES, VOID_ELEMENTS},
    expression::escape_html,
};

use crate::ir::{SSRContext, SSRResult};

/// Transform a native HTML/SVG element for SSR
pub fn transform_element<'a>(
    element: &JSXElement<'a>,
    tag_name: &str,
    context: &SSRContext,
    options: &TransformOptions<'a>,
) -> SSRResult {
    let is_void = VOID_ELEMENTS.contains(tag_name);
    let is_script_or_style = tag_name == "script" || tag_name == "style";

    let mut result = SSRResult::new();
    result.tag_name = Some(tag_name.to_string());
    result.skip_escape = is_script_or_style;

    // Check for spread attributes - need different handling
    let has_spread = element.opening_element.attributes.iter().any(|a| {
        matches!(a, JSXAttributeItem::SpreadAttribute(_))
    });

    if has_spread {
        return transform_element_with_spread(element, tag_name, context, options);
    }

    // Start the tag
    result.push_static(&format!("<{}", tag_name));

    // Add hydration key if needed
    if context.hydratable && options.hydratable {
        context.register_helper("ssrHydrationKey");
        result.push_dynamic("ssrHydrationKey()".to_string(), false, true);
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
    _element: &JSXElement<'a>,
    tag_name: &str,
    context: &SSRContext,
    options: &TransformOptions<'a>,
) -> SSRResult {
    context.register_helper("ssrElement");
    context.register_helper("escape");

    let mut result = SSRResult::new();
    result.has_spread = true;

    // For spread, we generate: ssrElement("tag", props, children, needsHydrationKey)
    // This is handled at code generation time
    result.push_dynamic(
        format!(
            "ssrElement(\"{}\", /* props */, /* children */, {})",
            tag_name,
            context.hydratable && options.hydratable
        ),
        false,
        true,
    );

    result
}

/// Transform element attributes for SSR
fn transform_attributes<'a>(
    element: &JSXElement<'a>,
    result: &mut SSRResult,
    context: &SSRContext,
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
    result: &mut SSRResult,
    context: &SSRContext,
    _options: &TransformOptions<'a>,
    is_svg: bool,
) {
    let key = get_attr_name(&attr.name);

    // Skip client-only attributes
    if key == "ref" || key.starts_with("on") || key.starts_with("use:") || key.starts_with("prop:") {
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
        ALIASES.get(key.as_str()).copied().unwrap_or(&key).to_string()
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
                let expr_str = expr_to_string(expr);
                context.register_helper("escape");

                // Handle special attributes
                if key == "style" {
                    context.register_helper("ssrStyle");
                    result.push_static(&format!(" {}=\"", attr_name));
                    result.push_dynamic(format!("ssrStyle({})", expr_str), false, true);
                    result.push_static("\"");
                } else if key == "class" || key == "className" {
                    result.push_static(&format!(" {}=\"", attr_name));
                    result.push_dynamic(expr_str, true, false);
                    result.push_static("\"");
                } else if key == "classList" {
                    context.register_helper("ssrClassList");
                    result.push_static(" class=\"");
                    result.push_dynamic(format!("ssrClassList({})", expr_str), false, true);
                    result.push_static("\"");
                } else if PROPERTIES.contains(key.as_str()) {
                    // Boolean attributes
                    context.register_helper("ssrAttribute");
                    result.push_dynamic(
                        format!("ssrAttribute(\"{}\", {}, true)", attr_name, expr_str),
                        false,
                        true,
                    );
                } else {
                    // Regular attribute
                    result.push_static(&format!(" {}=\"", attr_name));
                    result.push_dynamic(expr_str, true, false);
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
    result: &mut SSRResult,
    context: &SSRContext,
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
                        result.push_dynamic(expr_to_string(expr), false, true);
                        return;
                    }
                }
            } else if key == "textContent" || key == "innerText" {
                if let Some(JSXAttributeValue::ExpressionContainer(container)) = &attr.value {
                    if let Some(expr) = container.expression.as_expression() {
                        context.register_helper("escape");
                        result.push_dynamic(expr_to_string(expr), false, false);
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
    result: &mut SSRResult,
    skip_escape: bool,
    context: &SSRContext,
    options: &TransformOptions<'a>,
) {
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
                    let child_transformer = |child: &oxc_ast::ast::JSXChild<'a>| -> Option<SSRResult> {
                        match child {
                            oxc_ast::ast::JSXChild::Element(el) => {
                                let tag = common::get_tag_name(el);
                                Some(if common::is_component(&tag) {
                                    // For deeply nested components, use simple fallback
                                    let mut r = SSRResult::new();
                                    r.push_dynamic(format!("createComponent({}, {{}})", tag), false, false);
                                    r
                                } else {
                                    transform_element(el, &tag, context, options)
                                })
                            }
                            _ => None,
                        }
                    };
                    crate::component::transform_component(child_elem, &child_tag, context, options, &child_transformer)
                } else {
                    transform_element(child_elem, &child_tag, context, options)
                };
                result.merge(child_result);
            }

            oxc_ast::ast::JSXChild::ExpressionContainer(container) => {
                if let Some(expr) = container.expression.as_expression() {
                    let expr_str = expr_to_string(expr);
                    context.register_helper("escape");

                    if skip_escape {
                        // Inside script/style - don't escape
                        result.push_dynamic(expr_str, false, true);
                    } else {
                        // Normal content - escape
                        result.push_dynamic(expr_str, false, false);
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
