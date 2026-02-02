//! solid/self-closing-comp
//!
//! Disallow extra closing tags for components without children.

use oxc_ast::ast::{JSXChild, JSXOpeningElement};
use oxc_span::Span;
use serde::{Deserialize, Serialize};

use crate::diagnostic::{Diagnostic, Fix};
use crate::utils::{
    children_is_empty_or_multiline_whitespace, get_element_name, is_component, is_dom_element,
    is_void_element,
};
use crate::{RuleCategory, RuleMeta};

/// Which elements should be self-closing
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SelfClosingOption {
    /// All matching elements should self-close
    #[default]
    All,
    /// No elements should self-close
    None,
}

/// Which HTML elements should be self-closing
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HtmlSelfClosingOption {
    /// All HTML elements should self-close when empty
    #[default]
    All,
    /// Only void elements (br, hr, img, etc.) should self-close
    Void,
    /// No HTML elements should self-close
    None,
}

/// Configuration for self-closing-comp
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelfClosingCompConfig {
    /// Which Solid components should be self-closing when possible
    #[serde(default)]
    pub component: SelfClosingOption,
    /// Which native HTML elements should be self-closing when possible
    #[serde(default)]
    pub html: HtmlSelfClosingOption,
}

/// self-closing-comp rule
#[derive(Debug, Clone, Default)]
pub struct SelfClosingComp {
    pub config: SelfClosingCompConfig,
}

impl RuleMeta for SelfClosingComp {
    const NAME: &'static str = "self-closing-comp";
    const CATEGORY: RuleCategory = RuleCategory::Style;
}

impl SelfClosingComp {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_config(config: SelfClosingCompConfig) -> Self {
        Self { config }
    }

    /// Check if an element should be self-closing when possible
    fn should_be_self_closing(&self, opening: &JSXOpeningElement) -> bool {
        if is_component(opening) {
            matches!(self.config.component, SelfClosingOption::All)
        } else if let Some(name) = get_element_name(opening) {
            if is_dom_element(&name) {
                match self.config.html {
                    HtmlSelfClosingOption::All => true,
                    HtmlSelfClosingOption::Void => is_void_element(&name),
                    HtmlSelfClosingOption::None => false,
                }
            } else {
                true
            }
        } else {
            true
        }
    }

    /// Check a JSX element for self-closing issues
    ///
    /// Arguments:
    /// - `opening`: The JSX opening element
    /// - `children`: The children of the JSX element
    /// - `closing_span`: The span of the closing element (if any - None means self-closing)
    pub fn check<'a>(
        &self,
        opening: &JSXOpeningElement<'a>,
        children: &[JSXChild<'a>],
        closing_span: Option<Span>,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        let can_self_close = children_is_empty_or_multiline_whitespace(children);
        let is_self_closing = closing_span.is_none();

        if can_self_close {
            let should_self_close = self.should_be_self_closing(opening);

            if should_self_close && !is_self_closing {
                // Should be self-closing but isn't
                let mut diagnostic = Diagnostic::warning(
                    Self::NAME,
                    opening.span,
                    "Empty components are self-closing.",
                );

                // Add fix: replace `>...</tagName>` with ` />`
                if let Some(closing) = closing_span {
                    // Calculate the range from '>' to end of closing tag
                    let fix_start = opening.span.end - 1; // The '>' character
                    let fix_end = closing.end;
                    diagnostic = diagnostic.with_fix(
                        Fix::new(Span::new(fix_start, fix_end), " />")
                            .with_message("Make self-closing"),
                    );
                }

                diagnostics.push(diagnostic);
            } else if !should_self_close && is_self_closing {
                // Should NOT be self-closing but is
                if let Some(name) = get_element_name(opening) {
                    let diagnostic = Diagnostic::warning(
                        Self::NAME,
                        opening.span,
                        "This element should not be self-closing.",
                    )
                    .with_fix(
                        // Replace ` />` or `/>` with `></${tagName}>`
                        Fix::new(
                            Span::new(opening.span.end - 2, opening.span.end),
                            format!("></{}>", name),
                        )
                        .with_message("Add closing tag"),
                    );

                    diagnostics.push(diagnostic);
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
    fn test_config_defaults() {
        let config = SelfClosingCompConfig::default();
        assert_eq!(config.component, SelfClosingOption::All);
        assert_eq!(config.html, HtmlSelfClosingOption::All);
    }

    #[test]
    fn test_config_deserialize() {
        let json = r#"{"component": "none", "html": "void"}"#;
        let config: SelfClosingCompConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.component, SelfClosingOption::None);
        assert_eq!(config.html, HtmlSelfClosingOption::Void);
    }
}
