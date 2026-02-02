//! solid/prefer-show
//!
//! Enforce using Solid's `<Show />` component for conditionally showing content.

use oxc_ast::ast::{
    ConditionalExpression, Expression, JSXChild, JSXElement, JSXExpressionContainer,
    JSXFragment, LogicalExpression,
};
use oxc_span::{GetSpan, Span};

use crate::diagnostic::{Diagnostic, Fix};
use crate::{RuleCategory, RuleMeta};

/// prefer-show rule
#[derive(Debug, Clone, Default)]
pub struct PreferShow;

impl RuleMeta for PreferShow {
    const NAME: &'static str = "prefer-show";
    const CATEGORY: RuleCategory = RuleCategory::Style;
}

impl PreferShow {
    pub fn new() -> Self {
        Self
    }

    /// Check a JSX expression container for conditional expressions
    pub fn check_expression_container<'a>(
        &self,
        container: &JSXExpressionContainer<'a>,
        source: &str,
        parent_is_jsx: bool,
    ) -> Vec<Diagnostic> {
        if !parent_is_jsx {
            return Vec::new();
        }

        let mut diagnostics = Vec::new();

        let expr = match container.expression.as_expression() {
            Some(e) => e,
            None => return diagnostics,
        };

        match expr {
            Expression::LogicalExpression(logical) => {
                diagnostics.extend(self.check_logical_expression(
                    logical,
                    container.span,
                    source,
                ));
            }
            Expression::ArrowFunctionExpression(arrow) => {
                // For arrow functions, check if the body is an expression (not block body)
                if arrow.expression {
                    // Get the expression from the function body
                    if let Some(oxc_ast::ast::Statement::ExpressionStatement(expr_stmt)) =
                        arrow.body.statements.first()
                    {
                        match &expr_stmt.expression {
                            Expression::LogicalExpression(logical) => {
                                diagnostics.extend(self.check_logical_expression(
                                    logical,
                                    logical.span,
                                    source,
                                ));
                            }
                            Expression::ConditionalExpression(cond) => {
                                diagnostics.extend(self.check_conditional_expression(
                                    cond,
                                    cond.span,
                                    source,
                                ));
                            }
                            _ => {}
                        }
                    }
                }
            }
            Expression::ConditionalExpression(cond) => {
                diagnostics.extend(self.check_conditional_expression(
                    cond,
                    container.span,
                    source,
                ));
            }
            _ => {}
        }

        diagnostics
    }

    /// Check JSX element children for conditional expressions
    pub fn check_element_children<'a>(
        &self,
        element: &JSXElement<'a>,
        source: &str,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        for child in &element.children {
            if let JSXChild::ExpressionContainer(container) = child {
                diagnostics.extend(self.check_expression_container(container, source, true));
            }
        }

        diagnostics
    }

    /// Check JSX fragment children for conditional expressions
    pub fn check_fragment_children<'a>(
        &self,
        fragment: &JSXFragment<'a>,
        source: &str,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        for child in &fragment.children {
            if let JSXChild::ExpressionContainer(container) = child {
                diagnostics.extend(self.check_expression_container(container, source, true));
            }
        }

        diagnostics
    }

    fn check_logical_expression(
        &self,
        logical: &LogicalExpression<'_>,
        replace_span: Span,
        source: &str,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        // Only check && expressions
        if !matches!(logical.operator, oxc_syntax::operator::LogicalOperator::And) {
            return diagnostics;
        }

        // Check if right side is "expensive"
        if self.is_expensive_type(&logical.right) {
            let when_text = self.get_source_text(source, logical.left.span());
            let children_text = self.put_into_jsx(source, &logical.right);

            diagnostics.push(
                Diagnostic::warning(
                    Self::NAME,
                    logical.span,
                    "Use Solid's `<Show />` component for conditionally showing content.",
                )
                .with_fix(
                    Fix::new(
                        replace_span,
                        format!("<Show when={{{when_text}}}>{children_text}</Show>"),
                    )
                    .with_message("Convert to <Show /> component"),
                ),
            );
        }

        diagnostics
    }

    fn check_conditional_expression(
        &self,
        cond: &ConditionalExpression<'_>,
        replace_span: Span,
        source: &str,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        // Check if consequent or alternate is "expensive"
        if self.is_expensive_type(&cond.consequent) || self.is_expensive_type(&cond.alternate) {
            let when_text = self.get_source_text(source, cond.test.span());
            let children_text = self.put_into_jsx(source, &cond.consequent);
            let fallback_text = self.get_source_text(source, cond.alternate.span());

            diagnostics.push(
                Diagnostic::warning(
                    Self::NAME,
                    cond.span,
                    "Use Solid's `<Show />` component for conditionally showing content with a fallback.",
                )
                .with_fix(
                    Fix::new(
                        replace_span,
                        format!(
                            "<Show when={{{when_text}}} fallback={{{fallback_text}}}>{children_text}</Show>"
                        ),
                    )
                    .with_message("Convert to <Show /> component with fallback"),
                ),
            );
        }

        diagnostics
    }

    /// Check if expression is an "expensive" type
    fn is_expensive_type(&self, expr: &Expression<'_>) -> bool {
        matches!(
            expr,
            Expression::JSXElement(_)
                | Expression::JSXFragment(_)
                | Expression::Identifier(_)
        )
    }

    /// Convert expression to JSX-safe format
    fn put_into_jsx(&self, source: &str, expr: &Expression<'_>) -> String {
        let text = self.get_source_text(source, expr.span());
        if matches!(expr, Expression::JSXElement(_) | Expression::JSXFragment(_)) {
            text
        } else {
            format!("{{{text}}}")
        }
    }

    fn get_source_text(&self, source: &str, span: Span) -> String {
        source
            .get(span.start as usize..span.end as usize)
            .unwrap_or("")
            .to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rule_name() {
        assert_eq!(PreferShow::NAME, "prefer-show");
    }
}
