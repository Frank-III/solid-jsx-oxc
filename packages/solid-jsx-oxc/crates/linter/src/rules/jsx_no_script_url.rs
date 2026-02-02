//! solid/jsx-no-script-url
//!
//! Disallow `javascript:` URLs in JSX attributes.

use oxc_ast::ast::{
    Expression, JSXAttributeItem, JSXAttributeName, JSXAttributeValue, JSXOpeningElement,
};

use crate::diagnostic::Diagnostic;
use crate::{RuleCategory, RuleMeta};

/// jsx-no-script-url rule
#[derive(Debug, Clone, Default)]
pub struct JsxNoScriptUrl;

impl RuleMeta for JsxNoScriptUrl {
    const NAME: &'static str = "jsx-no-script-url";
    const CATEGORY: RuleCategory = RuleCategory::Correctness;
}

impl JsxNoScriptUrl {
    pub fn new() -> Self {
        Self
    }

    /// Check a JSX opening element for javascript: URLs
    pub fn check<'a>(&self, opening: &JSXOpeningElement<'a>) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        for attr in &opening.attributes {
            let JSXAttributeItem::Attribute(jsx_attr) = attr else {
                continue;
            };

            // Only check href and src attributes
            let attr_name = match &jsx_attr.name {
                JSXAttributeName::Identifier(ident) => ident.name.as_str(),
                _ => continue,
            };

            if !matches!(attr_name, "href" | "src" | "action" | "formAction") {
                continue;
            }

            if let Some(value) = &jsx_attr.value {
                if let Some(diagnostic) = self.check_value(value, jsx_attr.span, attr_name) {
                    diagnostics.push(diagnostic);
                }
            }
        }

        diagnostics
    }

    fn check_value(
        &self,
        value: &JSXAttributeValue,
        span: oxc_span::Span,
        attr_name: &str,
    ) -> Option<Diagnostic> {
        match value {
            JSXAttributeValue::StringLiteral(lit) => {
                let value_str = lit.value.as_str().trim();
                if value_str.to_lowercase().starts_with("javascript:") {
                    return Some(
                        Diagnostic::error(
                            Self::NAME,
                            span,
                            format!(
                                "`javascript:` URLs in the `{}` attribute are a security risk.",
                                attr_name
                            ),
                        )
                        .with_help("Use an event handler like `onClick` instead."),
                    );
                }
            }
            JSXAttributeValue::ExpressionContainer(container) => {
                // Check string expressions
                if let Some(expr) = container.expression.as_expression() {
                    if let Expression::StringLiteral(lit) = expr {
                        let value_str = lit.value.as_str().trim();
                        if value_str.to_lowercase().starts_with("javascript:") {
                            return Some(
                                Diagnostic::error(
                                    Self::NAME,
                                    span,
                                    format!(
                                        "`javascript:` URLs in the `{}` attribute are a security risk.",
                                        attr_name
                                    ),
                                )
                                .with_help("Use an event handler like `onClick` instead."),
                            );
                        }
                    }
                }
            }
            _ => {}
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rule_name() {
        assert_eq!(JsxNoScriptUrl::NAME, "jsx-no-script-url");
    }
}
