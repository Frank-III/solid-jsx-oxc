//! solid/prefer-classlist
//!
//! Enforce using the classlist prop over importing a classnames helper.
//! The classlist prop accepts an object `{ [class: string]: boolean }` just like classnames.

use oxc_ast::ast::{
    Argument, Expression, JSXAttributeName, JSXAttributeValue, JSXOpeningElement,
};
use oxc_span::Span;

use crate::diagnostic::{Diagnostic, Fix};
use crate::utils::has_attribute;
use crate::{RuleCategory, RuleMeta};

/// Default classnames helper function names
const DEFAULT_CLASSNAMES: &[&str] = &["cn", "clsx", "classnames"];

/// prefer-classlist rule
#[derive(Debug, Clone)]
pub struct PreferClasslist {
    /// Names to treat as classnames functions
    pub classnames: Vec<String>,
}

impl Default for PreferClasslist {
    fn default() -> Self {
        Self {
            classnames: DEFAULT_CLASSNAMES.iter().map(|s| s.to_string()).collect(),
        }
    }
}

impl RuleMeta for PreferClasslist {
    const NAME: &'static str = "prefer-classlist";
    const CATEGORY: RuleCategory = RuleCategory::Style;
}

impl PreferClasslist {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_classnames(mut self, classnames: Vec<String>) -> Self {
        self.classnames = classnames;
        self
    }

    /// Check a JSX opening element for classnames helper usage
    pub fn check<'a>(&self, opening: &JSXOpeningElement<'a>) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        // Skip if element already has a classlist prop
        if has_attribute(opening, "classlist") || has_attribute(opening, "classList") {
            return diagnostics;
        }

        for attr in &opening.attributes {
            if let oxc_ast::ast::JSXAttributeItem::Attribute(jsx_attr) = attr {
                let prop_name = match &jsx_attr.name {
                    JSXAttributeName::Identifier(ident) => ident.name.as_str(),
                    JSXAttributeName::NamespacedName(_) => continue,
                };

                // Only check class/className props
                if prop_name != "class" && prop_name != "className" {
                    continue;
                }

                // Check for expression container with classnames call
                if let Some(JSXAttributeValue::ExpressionContainer(container)) = &jsx_attr.value {
                    if let Some(expr) = container.expression.as_expression() {
                        if let Some((callee_name, object_span)) =
                            self.get_classnames_call_info(expr)
                        {
                            diagnostics.push(
                                Diagnostic::warning(
                                    Self::NAME,
                                    jsx_attr.span,
                                    format!(
                                        "The classlist prop should be used instead of {} to efficiently set classes based on an object.",
                                        callee_name
                                    ),
                                )
                                .with_fix(
                                    Fix::new(
                                        Span::new(jsx_attr.span.start, object_span.start),
                                        "classList={",
                                    )
                                    .with_message("Replace with classList prop"),
                                )
                                .with_fix(
                                    Fix::new(
                                        Span::new(object_span.end, jsx_attr.span.end),
                                        "}",
                                    )
                                    .with_message(""),
                                ),
                            );
                        }
                    }
                }
            }
        }

        diagnostics
    }

    /// Check if expression is a classnames helper call with a single object argument
    /// Returns (callee_name, object_span) if it matches
    fn get_classnames_call_info<'a>(
        &self,
        expr: &'a Expression<'a>,
    ) -> Option<(&'a str, Span)> {
        let call = match expr {
            Expression::CallExpression(call) => call,
            _ => return None,
        };

        // Check callee is an identifier matching our classnames list
        let callee_name = match &call.callee {
            Expression::Identifier(ident) => ident.name.as_str(),
            _ => return None,
        };

        if !self.classnames.iter().any(|cn| cn == callee_name) {
            return None;
        }

        // Check there's exactly one argument and it's an object expression
        if call.arguments.len() != 1 {
            return None;
        }

        let arg = match &call.arguments[0] {
            Argument::ObjectExpression(obj) => obj,
            _ => return None,
        };

        Some((callee_name, arg.span))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rule_name() {
        assert_eq!(PreferClasslist::NAME, "prefer-classlist");
    }

    #[test]
    fn test_default_classnames() {
        let rule = PreferClasslist::new();
        assert!(rule.classnames.contains(&"cn".to_string()));
        assert!(rule.classnames.contains(&"clsx".to_string()));
        assert!(rule.classnames.contains(&"classnames".to_string()));
    }
}
