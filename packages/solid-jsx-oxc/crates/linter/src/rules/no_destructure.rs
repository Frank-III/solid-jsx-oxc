//! solid/no-destructure
//!
//! Disallow destructuring props. In Solid, props must be used with property accesses
//! (`props.foo`) to preserve reactivity.

use oxc_ast::ast::{
    ArrowFunctionExpression, Expression, Function, FunctionBody, Statement,
};
use oxc_span::GetSpan;

use crate::diagnostic::Diagnostic;
use crate::{RuleCategory, RuleMeta};

/// no-destructure rule
#[derive(Debug, Clone, Default)]
pub struct NoDestructure;

impl RuleMeta for NoDestructure {
    const NAME: &'static str = "no-destructure";
    const CATEGORY: RuleCategory = RuleCategory::Correctness;
}

impl NoDestructure {
    pub fn new() -> Self {
        Self
    }

    /// Check a regular function for destructured props
    pub fn check_function<'a>(
        &self,
        func: &Function<'a>,
        has_jsx_in_body: bool,
        is_inside_jsx_expression: bool,
    ) -> Vec<Diagnostic> {
        if is_inside_jsx_expression {
            return Vec::new();
        }

        if !has_jsx_in_body {
            return Vec::new();
        }

        self.check_params(&func.params, func.params.span())
    }

    /// Check an arrow function for destructured props
    pub fn check_arrow<'a>(
        &self,
        arrow: &ArrowFunctionExpression<'a>,
        has_jsx_in_body: bool,
        is_inside_jsx_expression: bool,
    ) -> Vec<Diagnostic> {
        if is_inside_jsx_expression {
            return Vec::new();
        }

        if !has_jsx_in_body {
            return Vec::new();
        }

        self.check_params(&arrow.params, arrow.params.span())
    }

    fn check_params(
        &self,
        params: &oxc_ast::ast::FormalParameters,
        params_span: oxc_span::Span,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        // Only check functions with exactly one parameter
        if params.items.len() != 1 {
            return diagnostics;
        }

        let param = &params.items[0];

        // Check if the parameter is destructured (ObjectPattern)
        if param.pattern.is_destructuring_pattern() {
            diagnostics.push(
                Diagnostic::warning(
                    Self::NAME,
                    param.span,
                    "Destructuring component props breaks Solid's reactivity; use property access instead.",
                )
                .with_help("Use `props.propertyName` instead of destructuring."),
            );
        }

        diagnostics
    }

    /// Helper to check if a function body contains JSX
    pub fn body_has_jsx(body: &FunctionBody) -> bool {
        for stmt in &body.statements {
            if Self::statement_has_jsx(stmt) {
                return true;
            }
        }
        false
    }

    fn statement_has_jsx(stmt: &Statement) -> bool {
        match stmt {
            Statement::ExpressionStatement(expr_stmt) => Self::expression_has_jsx(&expr_stmt.expression),
            Statement::ReturnStatement(ret) => {
                ret.argument.as_ref().is_some_and(|e| Self::expression_has_jsx(e))
            }
            Statement::BlockStatement(block) => {
                block.body.iter().any(|s| Self::statement_has_jsx(s))
            }
            Statement::IfStatement(if_stmt) => {
                Self::statement_has_jsx(&if_stmt.consequent)
                    || if_stmt.alternate.as_ref().is_some_and(|s| Self::statement_has_jsx(s))
            }
            _ => false,
        }
    }

    fn expression_has_jsx(expr: &Expression) -> bool {
        match expr {
            Expression::JSXElement(_) | Expression::JSXFragment(_) => true,
            Expression::ParenthesizedExpression(paren) => Self::expression_has_jsx(&paren.expression),
            Expression::ConditionalExpression(cond) => {
                Self::expression_has_jsx(&cond.consequent)
                    || Self::expression_has_jsx(&cond.alternate)
            }
            Expression::LogicalExpression(logical) => {
                Self::expression_has_jsx(&logical.left) || Self::expression_has_jsx(&logical.right)
            }
            Expression::ArrowFunctionExpression(arrow) => {
                if arrow.expression {
                    if let Some(Statement::ExpressionStatement(expr_stmt)) = arrow.body.statements.first() {
                        return Self::expression_has_jsx(&expr_stmt.expression);
                    }
                }
                Self::body_has_jsx(&arrow.body)
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rule_name() {
        assert_eq!(NoDestructure::NAME, "no-destructure");
    }
}
