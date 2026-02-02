//! solid/event-handlers
//!
//! Enforce naming DOM element event handlers consistently and prevent Solid's analysis
//! from misunderstanding whether a prop should be an event handler.

use oxc_ast::ast::{
    Expression, JSXAttributeItem, JSXAttributeName, JSXAttributeValue, JSXElementName,
    JSXOpeningElement,
};
use serde::{Deserialize, Serialize};

use crate::diagnostic::{Diagnostic, Fix};
use crate::utils::is_dom_element;
use crate::{RuleCategory, RuleMeta};

/// Common DOM events with correct casing
const COMMON_EVENTS: &[&str] = &[
    "onAnimationEnd",
    "onAnimationIteration",
    "onAnimationStart",
    "onBeforeInput",
    "onBlur",
    "onChange",
    "onClick",
    "onContextMenu",
    "onCopy",
    "onCut",
    "onDblClick",
    "onDrag",
    "onDragEnd",
    "onDragEnter",
    "onDragExit",
    "onDragLeave",
    "onDragOver",
    "onDragStart",
    "onDrop",
    "onError",
    "onFocus",
    "onFocusIn",
    "onFocusOut",
    "onGotPointerCapture",
    "onInput",
    "onInvalid",
    "onKeyDown",
    "onKeyPress",
    "onKeyUp",
    "onLoad",
    "onLostPointerCapture",
    "onMouseDown",
    "onMouseEnter",
    "onMouseLeave",
    "onMouseMove",
    "onMouseOut",
    "onMouseOver",
    "onMouseUp",
    "onPaste",
    "onPointerCancel",
    "onPointerDown",
    "onPointerEnter",
    "onPointerLeave",
    "onPointerMove",
    "onPointerOut",
    "onPointerOver",
    "onPointerUp",
    "onReset",
    "onScroll",
    "onSelect",
    "onSubmit",
    "onToggle",
    "onTouchCancel",
    "onTouchEnd",
    "onTouchMove",
    "onTouchStart",
    "onTransitionEnd",
    "onWheel",
];

/// Configuration for event-handlers rule
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EventHandlersConfig {
    /// If true, don't warn on ambiguously named event handlers
    #[serde(default)]
    pub ignore_case: bool,
    /// If true, warn when spreading event handlers onto JSX
    #[serde(default)]
    pub warn_on_spread: bool,
}

/// event-handlers rule
#[derive(Debug, Clone, Default)]
pub struct EventHandlers {
    pub config: EventHandlersConfig,
}

impl RuleMeta for EventHandlers {
    const NAME: &'static str = "event-handlers";
    const CATEGORY: RuleCategory = RuleCategory::Correctness;
}

impl EventHandlers {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_config(config: EventHandlersConfig) -> Self {
        Self { config }
    }

    /// Check a JSX opening element for event handler issues
    pub fn check<'a>(&self, opening: &JSXOpeningElement<'a>) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        // Only check DOM elements
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

            // Skip namespaced attributes
            let (name, name_span) = match &jsx_attr.name {
                JSXAttributeName::Identifier(ident) => {
                    (ident.name.as_str(), ident.span)
                }
                JSXAttributeName::NamespacedName(_) => continue,
            };

            // Check if it looks like an event handler
            if !name.starts_with("on") || name.len() < 3 {
                continue;
            }

            let third_char = name.chars().nth(2).unwrap();
            if !third_char.is_ascii_alphabetic() {
                continue;
            }

            // Check for static values being treated as attributes
            if let Some(value) = &jsx_attr.value {
                if self.is_static_value(value) {
                    diagnostics.push(
                        Diagnostic::warning(
                            Self::NAME,
                            jsx_attr.span,
                            format!(
                                "The {} prop is named as an event handler but has a static value, so it will be treated as an attribute. Use attr:{} if intentional.",
                                name, name
                            ),
                        ),
                    );
                    continue;
                }
            }

            if self.config.ignore_case {
                continue;
            }

            let lowercase = name.to_lowercase();

            // Check for nonstandard events
            if lowercase == "ondoubleclick" {
                diagnostics.push(
                    Diagnostic::warning(Self::NAME, name_span, format!(
                        "The {} prop should be renamed to onDblClick, because it's not a standard event handler.",
                        name
                    ))
                    .with_fix(Fix::new(name_span, "onDblClick").with_message("Use standard name")),
                );
                continue;
            }

            // Check for common events with wrong casing
            if let Some(correct) = self.get_correct_event_name(&lowercase) {
                if correct != name {
                    diagnostics.push(
                        Diagnostic::warning(
                            Self::NAME,
                            name_span,
                            format!("The {} prop should be renamed to {} for readability.", name, correct),
                        )
                        .with_fix(Fix::new(name_span, correct.to_string()).with_message("Fix casing")),
                    );
                    continue;
                }
            }

            // Check for ambiguous naming (third char is lowercase)
            if third_char.is_ascii_lowercase() && self.get_correct_event_name(&lowercase).is_none() {
                let handler_name = format!(
                    "on{}{}",
                    third_char.to_ascii_uppercase(),
                    &name[3..]
                );
                let attr_name = format!("attr:{}", name);
                diagnostics.push(
                    Diagnostic::warning(
                        Self::NAME,
                        name_span,
                        format!(
                            "The {} prop is ambiguous. If it is an event handler, change it to {}. If it is an attribute, change it to {}.",
                            name, handler_name, attr_name
                        ),
                    ),
                );
            }
        }

        diagnostics
    }

    fn is_static_value(&self, value: &JSXAttributeValue) -> bool {
        match value {
            JSXAttributeValue::StringLiteral(_) => true,
            JSXAttributeValue::ExpressionContainer(container) => {
                if let Some(expr) = container.expression.as_expression() {
                    matches!(expr, Expression::StringLiteral(_) | Expression::NumericLiteral(_))
                } else {
                    false
                }
            }
            _ => false,
        }
    }

    fn get_correct_event_name(&self, lowercase: &str) -> Option<&'static str> {
        COMMON_EVENTS
            .iter()
            .find(|e| e.to_lowercase() == lowercase)
            .copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rule_name() {
        assert_eq!(EventHandlers::NAME, "event-handlers");
    }

    #[test]
    fn test_get_correct_event_name() {
        let rule = EventHandlers::new();
        assert_eq!(rule.get_correct_event_name("onclick"), Some("onClick"));
        assert_eq!(rule.get_correct_event_name("onmousedown"), Some("onMouseDown"));
        assert_eq!(rule.get_correct_event_name("onfoobar"), None);
    }
}
