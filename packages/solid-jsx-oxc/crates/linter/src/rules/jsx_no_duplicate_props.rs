//! solid/jsx-no-duplicate-props
//!
//! Disallow passing the same prop twice in JSX.
//!
//! In Solid, this also covers:
//! - Duplicate `class` props (use `classList` instead)
//! - Conflicting children sources (innerHTML, textContent, children prop, JSX children)

use oxc_ast::ast::{JSXAttributeItem, JSXAttributeName, JSXChild, JSXOpeningElement};
use oxc_span::Span;
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};

use crate::diagnostic::Diagnostic;
use crate::utils::{has_children, is_event_handler};
use crate::{RuleCategory, RuleMeta};

/// Configuration for jsx-no-duplicate-props
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JsxNoDuplicatePropsConfig {
    /// Consider two prop names differing only by case to be the same
    #[serde(default)]
    pub ignore_case: bool,
}

/// jsx-no-duplicate-props rule
#[derive(Debug, Clone, Default)]
pub struct JsxNoDuplicateProps {
    pub config: JsxNoDuplicatePropsConfig,
}

impl RuleMeta for JsxNoDuplicateProps {
    const NAME: &'static str = "jsx-no-duplicate-props";
    const CATEGORY: RuleCategory = RuleCategory::Correctness;
}

impl JsxNoDuplicateProps {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_config(config: JsxNoDuplicatePropsConfig) -> Self {
        Self { config }
    }

    /// Check a JSX opening element for duplicate props
    pub fn check<'a>(
        &self,
        opening: &JSXOpeningElement<'a>,
        children: &[JSXChild<'a>],
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let mut props: FxHashMap<String, Span> = FxHashMap::default();

        for attr in &opening.attributes {
            let JSXAttributeItem::Attribute(jsx_attr) = attr else {
                continue;
            };

            let (name, span) = match &jsx_attr.name {
                JSXAttributeName::Identifier(ident) => (ident.name.to_string(), ident.span),
                JSXAttributeName::NamespacedName(ns) => (
                    format!("{}:{}", ns.namespace.name, ns.name.name),
                    ns.span,
                ),
            };

            // Normalize name for comparison
            let normalized = if self.config.ignore_case || is_event_handler(&name) {
                name.to_lowercase()
                    .replace("oncapture:", "on")
                    .replace("on:", "on")
                    .replace("attr:", "")
                    .replace("prop:", "")
            } else {
                name.clone()
            };

            if let Some(first_span) = props.get(&normalized) {
                let (message, help) = if normalized == "class" {
                    (
                        "Duplicate `class` props are not allowed.".to_string(),
                        "While it might seem to work, it can break unexpectedly. Use `classList` instead.".to_string(),
                    )
                } else {
                    (
                        format!(
                            "No duplicate props allowed. The prop \"{}\" is duplicated.",
                            name
                        ),
                        "Remove one of the props, or rename them so each prop is distinct."
                            .to_string(),
                    )
                };

                diagnostics.push(
                    Diagnostic::warning(Self::NAME, span, message)
                        .with_help(help)
                        .with_label(*first_span, "first occurrence"),
                );
            } else {
                props.insert(normalized, span);
            }
        }

        // Check for conflicting children sources
        let has_children_prop = props.contains_key("children");
        let has_jsx_children = has_children(children);
        let has_inner_html =
            props.contains_key("innerHTML") || props.contains_key("innerhtml");
        let has_text_content =
            props.contains_key("textContent") || props.contains_key("textcontent");

        let mut used = Vec::new();
        if has_children_prop {
            used.push("`props.children`");
        }
        if has_jsx_children {
            used.push("JSX children");
        }
        if has_inner_html {
            used.push("`props.innerHTML`");
        }
        if has_text_content {
            used.push("`props.textContent`");
        }

        if used.len() > 1 {
            diagnostics.push(
                Diagnostic::warning(
                    Self::NAME,
                    opening.span,
                    format!("Using {} at the same time is not allowed.", used.join(", ")),
                )
                .with_help("Choose one method for setting element content."),
            );
        }

        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_defaults() {
        let config = JsxNoDuplicatePropsConfig::default();
        assert!(!config.ignore_case);
    }

    #[test]
    fn test_config_deserialize() {
        let json = r#"{"ignoreCase": true}"#;
        let config: JsxNoDuplicatePropsConfig = serde_json::from_str(json).unwrap();
        assert!(config.ignore_case);
    }
}
