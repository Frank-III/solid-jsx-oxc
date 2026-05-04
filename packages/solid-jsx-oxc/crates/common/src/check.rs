//! Check functions for JSX nodes
//! Ported from dom-expressions/src/shared/utils.js

use oxc_ast::ast::{
    Expression, JSXAttribute, JSXAttributeItem, JSXAttributeName, JSXAttributeValue, JSXElement,
    JSXElementName, JSXMemberExpression, JSXMemberExpressionObject,
};

use crate::constants::{BUILT_INS, SVG_ELEMENTS};
use crate::expression::expr_to_string;

/// Check if a tag name represents a component (starts with uppercase or contains dot)
pub fn is_component(tag: &str) -> bool {
    if tag.is_empty() {
        return false;
    }
    let first_char = tag.chars().next().unwrap();
    first_char.is_uppercase() || tag.contains('.')
}

/// Check if this is a built-in Solid component (For, Show, etc.)
pub fn is_built_in(tag: &str) -> bool {
    BUILT_INS.contains(tag)
}

/// Check if this is an SVG element
pub fn is_svg_element(tag: &str) -> bool {
    SVG_ELEMENTS.contains(tag)
}

/// Get the tag name from a JSX element
pub fn get_tag_name(element: &JSXElement) -> String {
    get_jsx_element_name(&element.opening_element.name)
}

/// Get the name from a JSXElementName
fn get_jsx_element_name(name: &JSXElementName) -> String {
    match name {
        JSXElementName::Identifier(id) => id.name.to_string(),
        JSXElementName::IdentifierReference(id) => id.name.to_string(),
        JSXElementName::NamespacedName(ns) => {
            format!("{}:{}", ns.namespace.name, ns.name.name)
        }
        JSXElementName::MemberExpression(member) => get_member_expression_name(member),
        JSXElementName::ThisExpression(_) => "this".to_string(),
    }
}

/// Get the name from a JSX member expression (e.g., Foo.Bar.Baz)
fn get_member_expression_name(member: &JSXMemberExpression) -> String {
    let object = match &member.object {
        JSXMemberExpressionObject::IdentifierReference(id) => id.name.to_string(),
        JSXMemberExpressionObject::MemberExpression(m) => get_member_expression_name(m),
        JSXMemberExpressionObject::ThisExpression(_) => "this".to_string(),
    };
    format!("{}.{}", object, member.property.name)
}

/// Check if an expression is dynamic (needs effect wrapping)
/// This is a simplified version - full implementation would need scope analysis
pub fn is_dynamic(expr: &Expression) -> bool {
    match expr {
        // Literals are static
        Expression::StringLiteral(_)
        | Expression::NumericLiteral(_)
        | Expression::BooleanLiteral(_)
        | Expression::NullLiteral(_) => false,

        // Template literals with no expressions are static
        Expression::TemplateLiteral(t) if t.expressions.is_empty() => false,

        // Function calls are dynamic
        Expression::CallExpression(_) => true,

        // Member expressions accessing reactive values are dynamic
        Expression::StaticMemberExpression(_) | Expression::ComputedMemberExpression(_) => true,

        // Identifiers need scope analysis, assume dynamic for now
        Expression::Identifier(_) => true,

        // Conditional expressions are dynamic
        Expression::ConditionalExpression(_) | Expression::LogicalExpression(_) => true,

        // Binary/unary with dynamic operands
        Expression::BinaryExpression(b) => is_dynamic(&b.left) || is_dynamic(&b.right),
        Expression::UnaryExpression(u) => is_dynamic(&u.argument),

        // Arrow functions themselves are static (the reference)
        Expression::ArrowFunctionExpression(_) | Expression::FunctionExpression(_) => false,

        // Object/array literals depend on their contents
        Expression::ObjectExpression(o) => o.properties.iter().any(|p| match p {
            oxc_ast::ast::ObjectPropertyKind::ObjectProperty(prop) => is_dynamic(&prop.value),
            oxc_ast::ast::ObjectPropertyKind::SpreadProperty(spread) => {
                is_dynamic(&spread.argument)
            }
        }),
        Expression::ArrayExpression(a) => a.elements.iter().any(|el| match el {
            oxc_ast::ast::ArrayExpressionElement::SpreadElement(s) => is_dynamic(&s.argument),
            oxc_ast::ast::ArrayExpressionElement::Elision(_) => false,
            _ => {
                if let Some(expr) = el.as_expression() {
                    is_dynamic(expr)
                } else {
                    false
                }
            }
        }),

        // Default to dynamic for safety
        _ => true,
    }
}

/// Babel-parity dynamism check used for spread-arg arrow-wrapping and for the
/// getter-vs-plain-prop decision when folding attributes into the merged-props
/// object.
///
/// Mirrors `isDynamic(path, { checkMember: true, checkCallExpressions: true })`
/// in `babel-plugin-jsx-dom-expressions/src/shared/utils.js`. The crucial
/// difference from the conservative `is_dynamic` above: a **plain Identifier
/// is NOT considered dynamic here**. Babel only treats an expression as
/// "dynamic for spread purposes" when it is one of:
///
/// - CallExpression / OptionalCallExpression / TaggedTemplateExpression
/// - MemberExpression (static or computed) / OptionalMemberExpression
/// - SpreadElement (recurse into argument)
/// - BinaryExpression with operator `in`
///
/// Anything else — including a bare identifier reference — is treated as
/// static. This matters because `mergeProps(rest, ...)` reads `rest`'s getters
/// lazily on every property access, which preserves reactivity for splitProps
/// proxies; wrapping it as `() => rest` and emitting `mergeProps(() => rest, ...)`
/// changes the access timing and shifts hydration-ID allocation order, which
/// surfaces as `Hydration Mismatch. Unable to find DOM nodes for hydration key`.
pub fn is_dynamic_for_spread(expr: &Expression) -> bool {
    use oxc_ast::ast::{ArrayExpressionElement, ObjectPropertyKind, PropertyKey};
    use oxc_syntax::operator::BinaryOperator;
    match expr {
        // Functions are static *references*; their bodies are not walked.
        Expression::ArrowFunctionExpression(_) | Expression::FunctionExpression(_) => false,

        // Direct dynamic forms.
        Expression::CallExpression(_) => true,
        Expression::TaggedTemplateExpression(_) => true,
        Expression::StaticMemberExpression(_)
        | Expression::ComputedMemberExpression(_)
        | Expression::PrivateFieldExpression(_) => true,
        Expression::ChainExpression(_) => true, // optional call / optional member
        Expression::UpdateExpression(_) => true,

        // BinaryExpression: `in` is dynamic by rule; otherwise recurse.
        Expression::BinaryExpression(b) => {
            matches!(b.operator, BinaryOperator::In)
                || is_dynamic_for_spread(&b.left)
                || is_dynamic_for_spread(&b.right)
        }
        Expression::LogicalExpression(l) => {
            is_dynamic_for_spread(&l.left) || is_dynamic_for_spread(&l.right)
        }
        Expression::ConditionalExpression(c) => {
            is_dynamic_for_spread(&c.test)
                || is_dynamic_for_spread(&c.consequent)
                || is_dynamic_for_spread(&c.alternate)
        }
        Expression::UnaryExpression(u) => is_dynamic_for_spread(&u.argument),
        Expression::AssignmentExpression(a) => is_dynamic_for_spread(&a.right),
        Expression::SequenceExpression(s) => s.expressions.iter().any(is_dynamic_for_spread),

        // Object / array / template literals: dynamic if any descendant is.
        Expression::ObjectExpression(o) => o.properties.iter().any(|p| match p {
            ObjectPropertyKind::ObjectProperty(prop) => {
                // Computed key may itself contain a dynamic expression.
                let key_dyn = if prop.computed {
                    match &prop.key {
                        PropertyKey::StaticIdentifier(_)
                        | PropertyKey::StringLiteral(_)
                        | PropertyKey::NumericLiteral(_) => false,
                        other => other
                            .as_expression()
                            .map(is_dynamic_for_spread)
                            .unwrap_or(false),
                    }
                } else {
                    false
                };
                key_dyn || is_dynamic_for_spread(&prop.value)
            }
            ObjectPropertyKind::SpreadProperty(spread) => is_dynamic_for_spread(&spread.argument),
        }),
        Expression::ArrayExpression(a) => a.elements.iter().any(|el| match el {
            ArrayExpressionElement::SpreadElement(s) => is_dynamic_for_spread(&s.argument),
            ArrayExpressionElement::Elision(_) => false,
            _ => el
                .as_expression()
                .map(is_dynamic_for_spread)
                .unwrap_or(false),
        }),
        Expression::TemplateLiteral(t) => t.expressions.iter().any(is_dynamic_for_spread),

        // TS wrappers / parens — recurse through.
        Expression::TSAsExpression(e) => is_dynamic_for_spread(&e.expression),
        Expression::TSSatisfiesExpression(e) => is_dynamic_for_spread(&e.expression),
        Expression::TSNonNullExpression(e) => is_dynamic_for_spread(&e.expression),
        Expression::TSTypeAssertion(e) => is_dynamic_for_spread(&e.expression),
        Expression::ParenthesizedExpression(e) => is_dynamic_for_spread(&e.expression),

        // Plain identifier, literals, `this`, `super`, JSX, etc. — static here.
        _ => false,
    }
}

/// Find a JSX attribute by name on an element.
///
/// Returns the attribute if found, allowing access to both the name and value.
pub fn find_prop<'a>(element: &'a JSXElement<'a>, name: &str) -> Option<&'a JSXAttribute<'a>> {
    for attr in &element.opening_element.attributes {
        if let JSXAttributeItem::Attribute(attr) = attr {
            if let JSXAttributeName::Identifier(id) = &attr.name {
                if id.name == name {
                    return Some(attr);
                }
            }
        }
    }
    None
}

/// Find a JSX attribute by name and return its value as a string.
///
/// Handles expression containers, string literals, and boolean attributes (no value = true).
pub fn find_prop_value(element: &JSXElement<'_>, name: &str) -> Option<String> {
    find_prop(element, name).and_then(|attr| get_attr_value(attr))
}

/// Get the value of a JSX attribute as a string.
///
/// - Expression containers: returns the expression as a string
/// - String literals: returns the quoted string
/// - No value (boolean): returns "true"
pub fn get_attr_value(attr: &JSXAttribute<'_>) -> Option<String> {
    match &attr.value {
        Some(JSXAttributeValue::ExpressionContainer(container)) => container
            .expression
            .as_expression()
            .map(|e| expr_to_string(e)),
        Some(JSXAttributeValue::StringLiteral(lit)) => Some(format!("\"{}\"", lit.value)),
        None => Some("true".to_string()),
        _ => None,
    }
}

/// Get the full name of a JSX attribute (including namespace if present).
///
/// - `id` -> "id"
/// - `on:click` -> "on:click"
pub fn get_attr_name(name: &JSXAttributeName) -> String {
    match name {
        JSXAttributeName::Identifier(id) => id.name.to_string(),
        JSXAttributeName::NamespacedName(ns) => {
            format!("{}:{}", ns.namespace.name, ns.name.name)
        }
    }
}

/// Check if a JSX attribute name is namespaced (e.g., `on:click`, `use:directive`).
pub fn is_namespaced_attr(name: &JSXAttributeName) -> bool {
    matches!(name, JSXAttributeName::NamespacedName(_))
}
