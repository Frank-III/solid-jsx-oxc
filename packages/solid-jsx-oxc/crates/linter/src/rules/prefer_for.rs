//! solid/prefer-for
//!
//! Enforce using Solid's `<For />` component for mapping an array to JSX elements.

use oxc_ast::ast::{
    Argument, CallExpression, ChainElement, Expression, JSXChild, JSXElement,
    JSXExpressionContainer, JSXFragment, MemberExpression,
};
use oxc_span::{GetSpan, Span};

use crate::diagnostic::{Diagnostic, Fix};
use crate::{RuleCategory, RuleMeta};

/// prefer-for rule
#[derive(Debug, Clone, Default)]
pub struct PreferFor;

impl RuleMeta for PreferFor {
    const NAME: &'static str = "prefer-for";
    const CATEGORY: RuleCategory = RuleCategory::Correctness;
}

impl PreferFor {
    pub fn new() -> Self {
        Self
    }

    /// Check a JSX expression container for Array.map usage
    pub fn check_expression_container<'a>(
        &self,
        container: &JSXExpressionContainer<'a>,
        container_span: Span,
        parent_is_jsx: bool,
    ) -> Vec<Diagnostic> {
        if !parent_is_jsx {
            return Vec::new();
        }

        let mut diagnostics = Vec::new();

        // Get the expression (handle ChainExpression)
        let expr = match container.expression.as_expression() {
            Some(e) => e,
            None => return diagnostics,
        };

        let call_expr = match expr {
            Expression::CallExpression(call) => call.as_ref(),
            Expression::ChainExpression(chain) => {
                if let ChainElement::CallExpression(call) = &chain.expression {
                    call.as_ref()
                } else {
                    return diagnostics;
                }
            }
            _ => return diagnostics,
        };

        // Check if it's a .map() call
        if let Some((array_span, map_fn_span, param_count)) = self.analyze_map_call(call_expr) {
            if param_count == 1 {
                // Only one param (no index), can safely use <For />
                diagnostics.push(
                    Diagnostic::warning(
                        Self::NAME,
                        call_expr.span,
                        "Use Solid's `<For />` component for efficiently rendering lists. Array#map causes DOM elements to be recreated.",
                    )
                    .with_fix(
                        Fix::new(
                            Span::new(container_span.start, array_span.start),
                            "<For each={",
                        )
                        .with_message("Convert to <For /> component"),
                    )
                    .with_fix(
                        Fix::new(
                            Span::new(array_span.end, map_fn_span.start),
                            "}>{",
                        )
                        .with_message(""),
                    )
                    .with_fix(
                        Fix::new(
                            Span::new(map_fn_span.end, container_span.end),
                            "}</For>",
                        )
                        .with_message(""),
                    ),
                );
            } else if param_count >= 2 {
                // Has index param, could be <For /> or <Index />
                diagnostics.push(
                    Diagnostic::warning(
                        Self::NAME,
                        call_expr.span,
                        "Use Solid's `<For />` component or `<Index />` component for rendering lists. Array#map causes DOM elements to be recreated.",
                    ),
                );
            }
        }

        diagnostics
    }

    /// Check JSX element children for map calls
    pub fn check_element_children<'a>(&self, element: &JSXElement<'a>) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        for child in &element.children {
            if let JSXChild::ExpressionContainer(container) = child {
                diagnostics.extend(self.check_expression_container(
                    container,
                    container.span,
                    true,
                ));
            }
        }

        diagnostics
    }

    /// Check JSX fragment children for map calls
    pub fn check_fragment_children<'a>(&self, fragment: &JSXFragment<'a>) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        for child in &fragment.children {
            if let JSXChild::ExpressionContainer(container) = child {
                diagnostics.extend(self.check_expression_container(
                    container,
                    container.span,
                    true,
                ));
            }
        }

        diagnostics
    }

    /// Analyze a call expression to see if it's arr.map(fn)
    /// Returns (array_span, map_fn_span, param_count) if it is
    fn analyze_map_call<'a>(
        &self,
        call: &'a CallExpression<'a>,
    ) -> Option<(Span, Span, usize)> {
        // Check it's a member expression call like arr.map(...)
        let member = call.callee.as_member_expression()?;

        // Check the property is "map"
        let prop_name = match member {
            MemberExpression::StaticMemberExpression(static_member) => {
                static_member.property.name.as_str()
            }
            MemberExpression::ComputedMemberExpression(computed) => {
                if let Expression::StringLiteral(lit) = &computed.expression {
                    lit.value.as_str()
                } else {
                    return None;
                }
            }
            _ => return None,
        };

        if prop_name != "map" {
            return None;
        }

        // Check there's exactly one argument (no thisArg)
        if call.arguments.len() != 1 {
            return None;
        }

        let map_fn = match &call.arguments[0] {
            Argument::SpreadElement(_) => return None,
            arg => arg.to_expression(),
        };

        // Check the argument is a function and get param count
        let param_count = match map_fn {
            Expression::ArrowFunctionExpression(arrow) => {
                if arrow.params.rest.is_some() {
                    return None; // Rest params, can't determine count
                }
                arrow.params.items.len()
            }
            Expression::FunctionExpression(func) => {
                if func.params.rest.is_some() {
                    return None;
                }
                func.params.items.len()
            }
            _ => return None,
        };

        let array_span = member.object().span();
        let map_fn_span = map_fn.span();

        Some((array_span, map_fn_span, param_count))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rule_name() {
        assert_eq!(PreferFor::NAME, "prefer-for");
    }
}
