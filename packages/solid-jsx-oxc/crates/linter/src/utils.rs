//! Utility functions for Solid linting rules

use oxc_ast::ast::{
    JSXAttribute, JSXAttributeItem, JSXAttributeName, JSXChild,
    JSXElementName, JSXMemberExpressionObject, JSXOpeningElement,
};
use oxc_span::Span;

/// Check if an element name is a DOM element (lowercase)
pub fn is_dom_element(name: &str) -> bool {
    name.chars().next().is_some_and(|c| c.is_lowercase())
}

/// Check if a JSX element name represents a component (capitalized or member expression)
pub fn is_component(opening: &JSXOpeningElement) -> bool {
    match &opening.name {
        JSXElementName::Identifier(ident) => !is_dom_element(&ident.name),
        JSXElementName::IdentifierReference(ident) => !is_dom_element(&ident.name),
        JSXElementName::MemberExpression(_) => true,
        JSXElementName::NamespacedName(_) => false,
        _ => false,
    }
}

/// Void HTML elements that don't have closing tags
pub const VOID_ELEMENTS: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source",
    "track", "wbr",
];

/// Check if a DOM element is a void element
pub fn is_void_element(name: &str) -> bool {
    VOID_ELEMENTS.contains(&name)
}

/// Solid built-in components
pub const SOLID_BUILTINS: &[&str] = &[
    "For",
    "Show",
    "Switch",
    "Match",
    "Index",
    "ErrorBoundary",
    "Suspense",
    "SuspenseList",
    "Dynamic",
    "Portal",
];

/// Check if a component is a Solid built-in
pub fn is_solid_builtin(name: &str) -> bool {
    SOLID_BUILTINS.contains(&name)
}

/// Get the name of a JSX element as a string
pub fn get_element_name(element: &JSXOpeningElement) -> Option<String> {
    match &element.name {
        JSXElementName::Identifier(ident) => Some(ident.name.to_string()),
        JSXElementName::IdentifierReference(ident) => Some(ident.name.to_string()),
        JSXElementName::NamespacedName(ns) => {
            Some(format!("{}:{}", ns.namespace.name, ns.name.name))
        }
        JSXElementName::MemberExpression(member) => {
            let mut parts = vec![member.property.name.to_string()];
            let mut current = &member.object;
            loop {
                match current {
                    JSXMemberExpressionObject::IdentifierReference(ident) => {
                        parts.push(ident.name.to_string());
                        break;
                    }
                    JSXMemberExpressionObject::MemberExpression(inner) => {
                        parts.push(inner.property.name.to_string());
                        current = &inner.object;
                    }
                    JSXMemberExpressionObject::ThisExpression(_) => {
                        parts.push("this".to_string());
                        break;
                    }
                }
            }
            parts.reverse();
            Some(parts.join("."))
        }
        _ => None,
    }
}

/// Get an attribute by name from a JSX opening element
pub fn get_attribute<'a>(
    element: &'a JSXOpeningElement<'a>,
    name: &str,
) -> Option<&'a JSXAttribute<'a>> {
    for attr in &element.attributes {
        if let JSXAttributeItem::Attribute(jsx_attr) = attr {
            if let JSXAttributeName::Identifier(ident) = &jsx_attr.name {
                if ident.name == name {
                    return Some(jsx_attr);
                }
            }
        }
    }
    None
}

/// Check if an element has an attribute with the given name
pub fn has_attribute(element: &JSXOpeningElement, name: &str) -> bool {
    get_attribute(element, name).is_some()
}

/// Get all attribute names and spans from a JSX opening element
pub fn get_all_attributes(element: &JSXOpeningElement) -> Vec<(String, Span)> {
    let mut result = Vec::new();
    for attr in &element.attributes {
        if let JSXAttributeItem::Attribute(jsx_attr) = attr {
            match &jsx_attr.name {
                JSXAttributeName::Identifier(ident) => {
                    result.push((ident.name.to_string(), ident.span));
                }
                JSXAttributeName::NamespacedName(ns) => {
                    result.push((
                        format!("{}:{}", ns.namespace.name, ns.name.name),
                        ns.span,
                    ));
                }
            }
        }
    }
    result
}

/// Check if the element has non-empty children
pub fn has_children(children: &[JSXChild]) -> bool {
    children.iter().any(|child| match child {
        JSXChild::Text(text) => !text.value.trim().is_empty(),
        JSXChild::Element(_) | JSXChild::Fragment(_) | JSXChild::ExpressionContainer(_) => true,
        _ => false,
    })
}

/// Check if children is empty or only whitespace with newlines
pub fn children_is_empty_or_multiline_whitespace(children: &[JSXChild]) -> bool {
    if children.is_empty() {
        return true;
    }
    if children.len() == 1 {
        if let JSXChild::Text(text) = &children[0] {
            return text.value.contains('\n') && text.value.chars().all(|c| c.is_whitespace());
        }
    }
    false
}

/// Check if a prop name is an event handler (starts with "on" + uppercase)
pub fn is_event_handler(name: &str) -> bool {
    name.starts_with("on")
        && name
            .chars()
            .nth(2)
            .is_some_and(|c| c.is_uppercase() || c == ':')
}

/// Normalize event handler name for comparison
pub fn normalize_event_name(name: &str) -> String {
    name.to_lowercase()
        .replace("oncapture:", "on")
        .replace("on:", "on")
}

/// Solid namespace prefixes
pub const SOLID_NAMESPACES: &[&str] = &[
    "on",
    "oncapture",
    "use",
    "prop",
    "attr",
    "bool",
    "class",
    "style",
];

/// Check if a namespace is valid for Solid
pub fn is_valid_namespace(ns: &str) -> bool {
    SOLID_NAMESPACES.contains(&ns)
}

/// React-specific props that should be replaced in Solid
pub const REACT_PROP_REPLACEMENTS: &[(&str, &str)] = &[
    ("className", "class"),
    ("htmlFor", "for"),
];

/// Get the Solid equivalent of a React prop, if applicable
pub fn get_solid_prop_replacement(react_prop: &str) -> Option<&'static str> {
    REACT_PROP_REPLACEMENTS
        .iter()
        .find(|(from, _)| *from == react_prop)
        .map(|(_, to)| *to)
}
