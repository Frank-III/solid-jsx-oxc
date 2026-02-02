//! solid/style-prop
//!
//! Require CSS properties in the `style` prop to be valid and kebab-cased.

use oxc_ast::ast::{
    Expression, JSXAttributeName, JSXAttributeValue, JSXOpeningElement, ObjectPropertyKind,
    PropertyKey,
};
use oxc_span::{GetSpan, Span};

use crate::diagnostic::{Diagnostic, Fix};
use crate::{RuleCategory, RuleMeta};

/// Common CSS length/percentage properties that shouldn't have numeric values
const LENGTH_PERCENTAGE_PROPS: &[&str] = &[
    "width",
    "height",
    "margin",
    "padding",
    "border-width",
    "font-size",
    "min-width",
    "max-width",
    "min-height",
    "max-height",
    "margin-top",
    "margin-right",
    "margin-bottom",
    "margin-left",
    "padding-top",
    "padding-right",
    "padding-bottom",
    "padding-left",
];

/// style-prop rule
#[derive(Debug, Clone)]
pub struct StyleProp {
    /// Prop names to treat as CSS style object
    pub style_props: Vec<String>,
    /// If true, allow string style values
    pub allow_string: bool,
}

impl Default for StyleProp {
    fn default() -> Self {
        Self {
            style_props: vec!["style".to_string()],
            allow_string: false,
        }
    }
}

impl RuleMeta for StyleProp {
    const NAME: &'static str = "style-prop";
    const CATEGORY: RuleCategory = RuleCategory::Correctness;
}

impl StyleProp {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_style_props(mut self, props: Vec<String>) -> Self {
        self.style_props = props;
        self
    }

    pub fn with_allow_string(mut self, allow: bool) -> Self {
        self.allow_string = allow;
        self
    }

    /// Check a JSX opening element for style prop issues
    pub fn check<'a>(&self, opening: &JSXOpeningElement<'a>) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        for attr in &opening.attributes {
            if let oxc_ast::ast::JSXAttributeItem::Attribute(jsx_attr) = attr {
                let prop_name = match &jsx_attr.name {
                    JSXAttributeName::Identifier(ident) => ident.name.to_string(),
                    JSXAttributeName::NamespacedName(_) => continue,
                };

                if !self.style_props.contains(&prop_name) {
                    continue;
                }

                // Get the style value
                let style_expr = match &jsx_attr.value {
                    Some(JSXAttributeValue::ExpressionContainer(container)) => {
                        container.expression.as_expression()
                    }
                    Some(JSXAttributeValue::StringLiteral(lit)) if !self.allow_string => {
                        // String style prop - warn
                        diagnostics.push(
                            Diagnostic::warning(
                                Self::NAME,
                                lit.span,
                                "Use an object for the style prop instead of a string.",
                            )
                            .with_fix(
                                Fix::new(
                                    jsx_attr.value.as_ref().map(|v| match v {
                                        JSXAttributeValue::StringLiteral(l) => l.span,
                                        _ => Span::default(),
                                    }).unwrap_or_default(),
                                    format!("{{{}}}", self.parse_style_string(&lit.value)),
                                )
                                .with_message("Convert to style object"),
                            ),
                        );
                        continue;
                    }
                    _ => continue,
                };

                let Some(expr) = style_expr else { continue };

                // Check for template literal (string)
                if let Expression::TemplateLiteral(_) = expr {
                    if !self.allow_string {
                        diagnostics.push(
                            Diagnostic::warning(
                                Self::NAME,
                                expr.span(),
                                "Use an object for the style prop instead of a string.",
                            ),
                        );
                    }
                    continue;
                }

                // Check for string literal in expression
                if let Expression::StringLiteral(lit) = expr {
                    if !self.allow_string {
                        diagnostics.push(
                            Diagnostic::warning(
                                Self::NAME,
                                lit.span,
                                "Use an object for the style prop instead of a string.",
                            )
                            .with_fix(
                                Fix::new(
                                    lit.span,
                                    self.parse_style_string(&lit.value),
                                )
                                .with_message("Convert to style object"),
                            ),
                        );
                    }
                    continue;
                }

                // Check object expression for CSS property issues
                if let Expression::ObjectExpression(obj) = expr {
                    for prop in &obj.properties {
                        if let ObjectPropertyKind::ObjectProperty(prop) = prop {
                            let (prop_name, key_span) = match &prop.key {
                                PropertyKey::StaticIdentifier(ident) => {
                                    (ident.name.to_string(), ident.span)
                                }
                                PropertyKey::StringLiteral(lit) => {
                                    (lit.value.to_string(), lit.span)
                                }
                                _ => continue,
                            };

                            // Skip CSS custom properties
                            if prop_name.starts_with("--") {
                                continue;
                            }

                            // Check for camelCase that should be kebab-case
                            let kebab_name = to_kebab_case(&prop_name);
                            if prop_name != kebab_name && is_valid_css_property(&kebab_name) {
                                diagnostics.push(
                                    Diagnostic::warning(
                                        Self::NAME,
                                        key_span,
                                        format!(
                                            "Use {} instead of {}.",
                                            kebab_name, prop_name
                                        ),
                                    )
                                    .with_fix(
                                        Fix::new(key_span, format!("\"{}\"", kebab_name))
                                            .with_message(format!(
                                                "Replace {} with {}",
                                                prop_name, kebab_name
                                            )),
                                    ),
                                );
                            } else if !is_valid_css_property(&prop_name)
                                && !is_valid_css_property(&kebab_name)
                            {
                                diagnostics.push(
                                    Diagnostic::warning(
                                        Self::NAME,
                                        key_span,
                                        format!("{} is not a valid CSS property.", prop_name),
                                    ),
                                );
                            }

                            // Check for numeric values on length/percentage properties
                            if is_length_percentage_property(&prop_name)
                                || is_length_percentage_property(&kebab_name)
                            {
                                if let Some(value) = get_numeric_value(&prop.value) {
                                    if value != 0.0 {
                                        diagnostics.push(
                                            Diagnostic::warning(
                                                Self::NAME,
                                                prop.value.span(),
                                                "This CSS property value should be a string with a unit; Solid does not automatically append a \"px\" unit.",
                                            ),
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        diagnostics
    }

    /// Parse a CSS style string into a JSON object string
    fn parse_style_string(&self, style: &str) -> String {
        let mut result = String::from("{");
        let parts: Vec<&str> = style.split(';').filter(|s| !s.trim().is_empty()).collect();

        for (i, part) in parts.iter().enumerate() {
            if let Some((key, value)) = part.split_once(':') {
                let key = key.trim();
                let value = value.trim();
                if i > 0 {
                    result.push_str(", ");
                }
                result.push_str(&format!("\"{}\": \"{}\"", key, value));
            }
        }

        result.push('}');
        result
    }
}

/// Convert camelCase to kebab-case
fn to_kebab_case(s: &str) -> String {
    let mut result = String::new();
    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() {
            if i > 0 {
                result.push('-');
            }
            result.push(c.to_ascii_lowercase());
        } else {
            result.push(c);
        }
    }
    result
}

/// Check if property is a valid CSS property (simplified check)
fn is_valid_css_property(name: &str) -> bool {
    // Common CSS properties - in a real implementation, use a comprehensive list
    const COMMON_CSS: &[&str] = &[
        "display", "position", "top", "right", "bottom", "left", "width", "height",
        "min-width", "max-width", "min-height", "max-height", "margin", "margin-top",
        "margin-right", "margin-bottom", "margin-left", "padding", "padding-top",
        "padding-right", "padding-bottom", "padding-left", "border", "border-width",
        "border-style", "border-color", "border-radius", "background", "background-color",
        "color", "font", "font-size", "font-weight", "font-family", "font-style",
        "line-height", "text-align", "text-decoration", "text-transform", "white-space",
        "overflow", "overflow-x", "overflow-y", "visibility", "opacity", "z-index",
        "flex", "flex-direction", "flex-wrap", "flex-grow", "flex-shrink", "flex-basis",
        "justify-content", "align-items", "align-content", "align-self", "gap", "grid",
        "grid-template-columns", "grid-template-rows", "grid-column", "grid-row",
        "transform", "transition", "animation", "cursor", "pointer-events", "user-select",
        "box-shadow", "box-sizing", "outline", "resize", "object-fit", "object-position",
    ];
    COMMON_CSS.contains(&name)
}

/// Check if property is a length/percentage property
fn is_length_percentage_property(name: &str) -> bool {
    LENGTH_PERCENTAGE_PROPS
        .iter()
        .any(|p| name.contains(p))
}

/// Get numeric value from expression
fn get_numeric_value(expr: &Expression<'_>) -> Option<f64> {
    match expr {
        Expression::NumericLiteral(lit) => Some(lit.value),
        Expression::UnaryExpression(unary) => {
            if let Expression::NumericLiteral(lit) = &unary.argument {
                Some(-lit.value)
            } else {
                None
            }
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rule_name() {
        assert_eq!(StyleProp::NAME, "style-prop");
    }

    #[test]
    fn test_to_kebab_case() {
        assert_eq!(to_kebab_case("fontSize"), "font-size");
        assert_eq!(to_kebab_case("backgroundColor"), "background-color");
        assert_eq!(to_kebab_case("borderTopWidth"), "border-top-width");
        assert_eq!(to_kebab_case("color"), "color");
    }

    #[test]
    fn test_is_valid_css_property() {
        assert!(is_valid_css_property("font-size"));
        assert!(is_valid_css_property("color"));
        assert!(is_valid_css_property("display"));
        assert!(!is_valid_css_property("invalidProp"));
    }
}
