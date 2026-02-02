//! solid/no-unknown-namespaces
//!
//! Enforce using only Solid-specific namespaced attribute names.

use oxc_ast::ast::{JSXAttributeItem, JSXAttributeName, JSXElementName, JSXOpeningElement};

use crate::diagnostic::{Diagnostic, Fix};
use crate::utils::is_dom_element;
use crate::{RuleCategory, RuleMeta};

/// Known Solid namespace prefixes
const KNOWN_NAMESPACES: &[&str] = &["on", "oncapture", "use", "prop", "attr", "bool"];

/// Style-related namespaces (confusing, prefer the prop instead)
const STYLE_NAMESPACES: &[&str] = &["style", "class"];

/// Other valid XML namespaces
const OTHER_NAMESPACES: &[&str] = &["xmlns", "xlink"];

/// no-unknown-namespaces rule
#[derive(Debug, Clone, Default)]
pub struct NoUnknownNamespaces {
    /// Additional namespace names to allow
    pub allowed_namespaces: Vec<String>,
}

impl RuleMeta for NoUnknownNamespaces {
    const NAME: &'static str = "no-unknown-namespaces";
    const CATEGORY: RuleCategory = RuleCategory::Correctness;
}

impl NoUnknownNamespaces {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_allowed_namespaces(mut self, namespaces: Vec<String>) -> Self {
        self.allowed_namespaces = namespaces;
        self
    }

    /// Check a JSX opening element for unknown namespaces
    pub fn check<'a>(&self, opening: &JSXOpeningElement<'a>) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        // Get element name to check if it's a component
        let is_component_element = match &opening.name {
            JSXElementName::Identifier(ident) => !is_dom_element(&ident.name),
            JSXElementName::IdentifierReference(ident) => !is_dom_element(&ident.name),
            JSXElementName::MemberExpression(_) => true,
            JSXElementName::NamespacedName(_) => false,
            _ => false,
        };

        for attr in &opening.attributes {
            if let JSXAttributeItem::Attribute(jsx_attr) = attr {
                if let JSXAttributeName::NamespacedName(ns) = &jsx_attr.name {
                    let namespace = ns.namespace.name.as_str();
                    let name = ns.name.name.as_str();

                    // Namespaced props have no effect on components
                    if is_component_element {
                        diagnostics.push(
                            Diagnostic::warning(
                                Self::NAME,
                                ns.span,
                                "Namespaced props have no effect on components.",
                            )
                            .with_fix(
                                Fix::new(ns.span, name.to_string())
                                    .with_message(format!("Replace {}:{} with {}", namespace, name, name)),
                            ),
                        );
                        continue;
                    }

                    // Check if namespace is allowed
                    let is_known = KNOWN_NAMESPACES.contains(&namespace)
                        || OTHER_NAMESPACES.contains(&namespace)
                        || self.allowed_namespaces.iter().any(|n| n == namespace);

                    if !is_known {
                        if STYLE_NAMESPACES.contains(&namespace) {
                            diagnostics.push(
                                Diagnostic::warning(
                                    Self::NAME,
                                    ns.span,
                                    format!(
                                        "Using the '{}:' special prefix is potentially confusing, prefer the '{}' prop instead.",
                                        namespace, namespace
                                    ),
                                ),
                            );
                        } else {
                            let known_list = KNOWN_NAMESPACES
                                .iter()
                                .map(|n| format!("'{}:'", n))
                                .collect::<Vec<_>>()
                                .join(", ");
                            diagnostics.push(
                                Diagnostic::warning(
                                    Self::NAME,
                                    ns.span,
                                    format!(
                                        "'{}:' is not one of Solid's special prefixes for JSX attributes ({}).",
                                        namespace, known_list
                                    ),
                                ),
                            );
                        }
                    }
                }
            }
        }

        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rule_name() {
        assert_eq!(NoUnknownNamespaces::NAME, "no-unknown-namespaces");
    }
}
