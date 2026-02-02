//! solid/no-innerhtml
//!
//! Disallow usage of the innerHTML attribute, which can often lead to security vulnerabilities.

use oxc_ast::ast::{
    Expression, JSXAttributeName, JSXAttributeValue, JSXElement, ObjectPropertyKind,
    PropertyKey,
};
use oxc_span::{GetSpan, Span};

use crate::diagnostic::{Diagnostic, Fix};
use crate::utils::has_children;
use crate::{RuleCategory, RuleMeta};

/// no-innerhtml rule
#[derive(Debug, Clone)]
pub struct NoInnerhtml {
    /// If the innerHTML value is guaranteed to be a static HTML string, allow it
    pub allow_static: bool,
}

impl Default for NoInnerhtml {
    fn default() -> Self {
        Self { allow_static: true }
    }
}

impl RuleMeta for NoInnerhtml {
    const NAME: &'static str = "no-innerhtml";
    const CATEGORY: RuleCategory = RuleCategory::Correctness;
}

impl NoInnerhtml {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_allow_static(mut self, allow_static: bool) -> Self {
        self.allow_static = allow_static;
        self
    }

    /// Check a JSX element for innerHTML usage
    pub fn check<'a>(&self, element: &JSXElement<'a>) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let opening = &element.opening_element;

        for attr in &opening.attributes {
            if let oxc_ast::ast::JSXAttributeItem::Attribute(jsx_attr) = attr {
                let prop_name = match &jsx_attr.name {
                    JSXAttributeName::Identifier(ident) => ident.name.as_str(),
                    JSXAttributeName::NamespacedName(_) => continue,
                };

                // Check for React's dangerouslySetInnerHTML
                if prop_name == "dangerouslySetInnerHTML" {
                    diagnostics.push(self.handle_dangerously_set_inner_html(
                        jsx_attr.span,
                        &jsx_attr.value,
                    ));
                    continue;
                }

                if prop_name != "innerHTML" {
                    continue;
                }

                // Check innerHTML usage
                if self.allow_static {
                    if let Some(static_value) = get_static_string_value(&jsx_attr.value) {
                        // Check if it looks like HTML
                        if looks_like_html(&static_value) {
                            // Check for conflict with children
                            if has_children(&element.children) {
                                diagnostics.push(
                                    Diagnostic::warning(
                                        Self::NAME,
                                        element.span,
                                        "The innerHTML attribute should not be used on an element with child elements; they will be overwritten.",
                                    ),
                                );
                            }
                            // Static HTML is allowed
                        } else {
                            // Doesn't look like HTML, suggest innerText
                            let attr_name_span = match &jsx_attr.name {
                                JSXAttributeName::Identifier(ident) => ident.span,
                                _ => jsx_attr.span,
                            };
                            diagnostics.push(
                                Diagnostic::warning(
                                    Self::NAME,
                                    jsx_attr.span,
                                    "The string passed to innerHTML does not appear to be valid HTML.",
                                )
                                .with_fix(
                                    Fix::new(attr_name_span, "innerText")
                                        .with_message("Use innerText for text content"),
                                ),
                            );
                        }
                    } else {
                        // Dynamic value - warn about security
                        diagnostics.push(
                            Diagnostic::warning(
                                Self::NAME,
                                jsx_attr.span,
                                "The innerHTML attribute is dangerous; passing unsanitized input can lead to security vulnerabilities.",
                            ),
                        );
                    }
                } else {
                    // allowStatic is false, always warn
                    diagnostics.push(
                        Diagnostic::warning(
                            Self::NAME,
                            jsx_attr.span,
                            "The innerHTML attribute is dangerous; passing unsanitized input can lead to security vulnerabilities.",
                        ),
                    );
                }
            }
        }

        diagnostics
    }

    fn handle_dangerously_set_inner_html(
        &self,
        attr_span: Span,
        value: &Option<JSXAttributeValue<'_>>,
    ) -> Diagnostic {
        // Check if it's the pattern: dangerouslySetInnerHTML={{ __html: value }}
        if let Some(JSXAttributeValue::ExpressionContainer(container)) = value {
            if let Some(Expression::ObjectExpression(obj)) = container.expression.as_expression() {
                if obj.properties.len() == 1 {
                    if let Some(ObjectPropertyKind::ObjectProperty(prop)) =
                        obj.properties.first()
                    {
                        if let PropertyKey::StaticIdentifier(key) = &prop.key {
                            if key.name == "__html" {
                                // Can provide a fix
                                return Diagnostic::warning(
                                    Self::NAME,
                                    attr_span,
                                    "The dangerouslySetInnerHTML prop is not supported; use innerHTML instead.",
                                )
                                .with_fix(
                                    Fix::new(
                                        Span::new(attr_span.start, prop.value.span().start),
                                        "innerHTML={",
                                    )
                                    .with_message("Replace dangerouslySetInnerHTML with innerHTML"),
                                )
                                .with_fix(
                                    Fix::new(
                                        Span::new(prop.value.span().end, attr_span.end),
                                        "}",
                                    )
                                    .with_message(""),
                                );
                            }
                        }
                    }
                }
            }
        }

        Diagnostic::warning(
            Self::NAME,
            attr_span,
            "The dangerouslySetInnerHTML prop is not supported; use innerHTML instead.",
        )
    }
}

/// Get static string value from JSX attribute value
fn get_static_string_value(value: &Option<JSXAttributeValue<'_>>) -> Option<String> {
    match value {
        Some(JSXAttributeValue::StringLiteral(lit)) => Some(lit.value.to_string()),
        Some(JSXAttributeValue::ExpressionContainer(container)) => {
            if let Some(expr) = container.expression.as_expression() {
                match expr {
                    Expression::StringLiteral(lit) => Some(lit.value.to_string()),
                    Expression::TemplateLiteral(tpl) if tpl.expressions.is_empty() => {
                        // Get text from template literal quasis
                        tpl.quasis.first().map(|q| q.value.raw.to_string())
                    }
                    _ => None,
                }
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Simple check if a string looks like HTML
fn looks_like_html(s: &str) -> bool {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return false;
    }
    // Check for HTML-like patterns: <tag>, &entity;, etc.
    trimmed.contains('<') && trimmed.contains('>')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rule_name() {
        assert_eq!(NoInnerhtml::NAME, "no-innerhtml");
    }

    #[test]
    fn test_looks_like_html() {
        assert!(looks_like_html("<div>test</div>"));
        assert!(looks_like_html("<br/>"));
        assert!(looks_like_html("Hello <b>world</b>"));
        assert!(!looks_like_html("plain text"));
        assert!(!looks_like_html(""));
    }
}
