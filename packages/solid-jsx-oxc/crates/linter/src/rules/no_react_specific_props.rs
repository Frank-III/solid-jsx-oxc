//! solid/no-react-specific-props
//!
//! Disallow usage of React-specific `className`/`htmlFor` props.
//! Also detects useless `key` props on DOM elements.

use oxc_ast::ast::{JSXAttributeName, JSXOpeningElement};

use crate::diagnostic::{Diagnostic, Fix};
use crate::utils::{get_attribute, get_element_name, has_attribute, is_dom_element};
use crate::{RuleCategory, RuleMeta};

/// no-react-specific-props rule
#[derive(Debug, Clone, Default)]
pub struct NoReactSpecificProps;

impl RuleMeta for NoReactSpecificProps {
    const NAME: &'static str = "no-react-specific-props";
    const CATEGORY: RuleCategory = RuleCategory::Correctness;
}

/// React-specific props and their Solid equivalents
const REACT_PROP_REPLACEMENTS: &[(&str, &str)] = &[("className", "class"), ("htmlFor", "for")];

impl NoReactSpecificProps {
    pub fn new() -> Self {
        Self
    }

    /// Check a JSX opening element for React-specific props
    pub fn check<'a>(&self, opening: &JSXOpeningElement<'a>) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        for (from, to) in REACT_PROP_REPLACEMENTS {
            if let Some(attr) = get_attribute(opening, from) {
                let attr_span = match &attr.name {
                    JSXAttributeName::Identifier(ident) => ident.span,
                    _ => attr.span,
                };

                let mut diagnostic = Diagnostic::warning(
                    Self::NAME,
                    attr.span,
                    format!(
                        "Prefer the `{}` prop over the deprecated `{}` prop.",
                        to, from
                    ),
                );

                // Only auto-fix if target prop doesn't already exist
                if !has_attribute(opening, to) {
                    diagnostic = diagnostic.with_fix(Fix::new(attr_span, to.to_string())
                        .with_message(format!("Replace `{}` with `{}`", from, to)));
                }

                diagnostics.push(diagnostic);
            }
        }

        // Check for useless `key` prop on DOM elements
        if let Some(name) = get_element_name(opening) {
            if is_dom_element(&name) {
                if let Some(key_attr) = get_attribute(opening, "key") {
                    diagnostics.push(
                        Diagnostic::warning(
                            Self::NAME,
                            key_attr.span,
                            "Elements in a <For> or <Index> list do not need a key prop.",
                        )
                        .with_help("Solid uses a different reconciliation strategy than React.")
                        .with_fix(Fix::new(key_attr.span, String::new())
                            .with_message("Remove `key` prop")),
                    );
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
        assert_eq!(NoReactSpecificProps::NAME, "no-react-specific-props");
    }
}
