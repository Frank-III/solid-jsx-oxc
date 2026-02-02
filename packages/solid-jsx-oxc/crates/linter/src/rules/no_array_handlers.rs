//! solid/no-array-handlers
//!
//! Disallow usage of type-unsafe event handlers (passing arrays).

use oxc_ast::ast::{
    Expression, JSXAttributeItem, JSXAttributeName, JSXAttributeValue, JSXElementName,
    JSXOpeningElement,
};

use crate::diagnostic::Diagnostic;
use crate::utils::is_dom_element;
use crate::{RuleCategory, RuleMeta};

/// no-array-handlers rule
#[derive(Debug, Clone, Default)]
pub struct NoArrayHandlers;

impl RuleMeta for NoArrayHandlers {
    const NAME: &'static str = "no-array-handlers";
    const CATEGORY: RuleCategory = RuleCategory::Correctness;
}

impl NoArrayHandlers {
    pub fn new() -> Self {
        Self
    }

    /// Check a JSX opening element for array event handlers
    pub fn check<'a>(&self, opening: &JSXOpeningElement<'a>) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        // Only check DOM elements (lowercase tag names)
        let element_name = match &opening.name {
            JSXElementName::Identifier(ident) => ident.name.as_str(),
            JSXElementName::IdentifierReference(ident) => ident.name.as_str(),
            _ => return diagnostics,
        };

        if !is_dom_element(element_name) {
            return diagnostics;
        }

        for attr in &opening.attributes {
            let JSXAttributeItem::Attribute(jsx_attr) = attr else {
                continue;
            };

            // Check if it's an event handler
            let is_event_handler = match &jsx_attr.name {
                JSXAttributeName::Identifier(ident) => {
                    let name = ident.name.as_str();
                    // Matches onX where X is uppercase
                    name.starts_with("on")
                        && name.chars().nth(2).is_some_and(|c| c.is_ascii_alphabetic())
                }
                JSXAttributeName::NamespacedName(ns) => {
                    // Matches on:* namespace
                    ns.namespace.name == "on"
                }
            };

            if !is_event_handler {
                continue;
            }

            // Check if value is an array expression
            if let Some(JSXAttributeValue::ExpressionContainer(container)) = &jsx_attr.value {
                if let Some(expr) = container.expression.as_expression() {
                    if matches!(expr, Expression::ArrayExpression(_)) {
                        diagnostics.push(Diagnostic::warning(
                            Self::NAME,
                            jsx_attr.span,
                            "Passing an array as an event handler is potentially type-unsafe.",
                        ));
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
        assert_eq!(NoArrayHandlers::NAME, "no-array-handlers");
    }
}
