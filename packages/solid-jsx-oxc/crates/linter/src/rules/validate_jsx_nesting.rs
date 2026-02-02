//! solid/validate-jsx-nesting
//!
//! Validate proper HTML element nesting in JSX.
//!
//! Note: The original eslint-plugin-solid rule was a stub (`export {}`).
//! This implementation provides basic validation of common nesting issues.

use oxc_ast::ast::{JSXChild, JSXElement, JSXElementName};

use crate::diagnostic::Diagnostic;
use crate::{RuleCategory, RuleMeta};

/// validate-jsx-nesting rule
#[derive(Debug, Clone, Default)]
pub struct ValidateJsxNesting;

impl RuleMeta for ValidateJsxNesting {
    const NAME: &'static str = "validate-jsx-nesting";
    const CATEGORY: RuleCategory = RuleCategory::Correctness;
}

/// Elements that can only contain specific children
const RESTRICTIVE_PARENTS: &[(&str, &[&str])] = &[
    ("ul", &["li", "script", "template"]),
    ("ol", &["li", "script", "template"]),
    ("table", &["caption", "colgroup", "thead", "tbody", "tfoot", "tr", "script", "template"]),
    ("thead", &["tr", "script", "template"]),
    ("tbody", &["tr", "script", "template"]),
    ("tfoot", &["tr", "script", "template"]),
    ("tr", &["th", "td", "script", "template"]),
    ("select", &["option", "optgroup", "script", "template"]),
    ("optgroup", &["option", "script", "template"]),
    ("dl", &["dt", "dd", "div", "script", "template"]),
    ("colgroup", &["col", "template"]),
];

/// Elements that cannot be nested inside themselves
const NO_SELF_NESTING: &[&str] = &[
    "a", "button", "form", "label", "p", "h1", "h2", "h3", "h4", "h5", "h6",
];

/// Block-level elements that cannot be inside inline elements
const BLOCK_ELEMENTS: &[&str] = &[
    "address", "article", "aside", "blockquote", "details", "dialog", "dd", "div", "dl", "dt",
    "fieldset", "figcaption", "figure", "footer", "form", "h1", "h2", "h3", "h4", "h5", "h6",
    "header", "hgroup", "hr", "li", "main", "nav", "ol", "p", "pre", "section", "table", "ul",
];

/// Inline elements that cannot contain block-level elements
const INLINE_ELEMENTS: &[&str] = &[
    "a", "abbr", "b", "bdi", "bdo", "cite", "code", "data", "dfn", "em", "i", "kbd", "mark", "q",
    "rp", "rt", "ruby", "s", "samp", "small", "span", "strong", "sub", "sup", "time", "u", "var",
];

impl ValidateJsxNesting {
    pub fn new() -> Self {
        Self
    }

    /// Check a JSX element for nesting violations
    pub fn check<'a>(
        &self,
        element: &JSXElement<'a>,
        parent_name: Option<&str>,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        let element_name = match &element.opening_element.name {
            JSXElementName::Identifier(ident) => ident.name.as_str(),
            JSXElementName::IdentifierReference(ident) => ident.name.as_str(),
            _ => return diagnostics,
        };

        // Skip components (capitalized)
        if element_name.chars().next().is_some_and(|c| c.is_uppercase()) {
            return diagnostics;
        }

        // Check if parent restricts children
        if let Some(parent) = parent_name {
            // Check restrictive parents
            if let Some((_, allowed)) = RESTRICTIVE_PARENTS.iter().find(|(p, _)| *p == parent) {
                if !allowed.contains(&element_name) && !element_name.chars().next().is_some_and(|c| c.is_uppercase()) {
                    diagnostics.push(Diagnostic::warning(
                        Self::NAME,
                        element.opening_element.span,
                        format!("<{}> cannot be a child of <{}>.", element_name, parent),
                    ));
                }
            }

            // Check no self-nesting
            if NO_SELF_NESTING.contains(&parent) && parent == element_name {
                diagnostics.push(Diagnostic::warning(
                    Self::NAME,
                    element.opening_element.span,
                    format!("<{}> cannot be nested inside another <{}>.", element_name, parent),
                ));
            }

            // Check block inside inline
            if INLINE_ELEMENTS.contains(&parent) && BLOCK_ELEMENTS.contains(&element_name) {
                diagnostics.push(Diagnostic::warning(
                    Self::NAME,
                    element.opening_element.span,
                    format!(
                        "Block element <{}> cannot be a child of inline element <{}>.",
                        element_name, parent
                    ),
                ));
            }
        }

        // Recursively check children
        for child in &element.children {
            if let JSXChild::Element(child_elem) = child {
                diagnostics.extend(self.check(child_elem, Some(element_name)));
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
        assert_eq!(ValidateJsxNesting::NAME, "validate-jsx-nesting");
    }
}
