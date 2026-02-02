//! solid/components-return-once
//!
//! Disallow early returns in components. Solid components only run once,
//! and so conditionals should be inside JSX.

use oxc_ast::ast::{
    ArrowFunctionExpression, Expression, Function, FunctionBody, Statement,
};

use crate::diagnostic::Diagnostic;
use crate::{RuleCategory, RuleMeta};

/// components-return-once rule
#[derive(Debug, Clone, Default)]
pub struct ComponentsReturnOnce;

impl RuleMeta for ComponentsReturnOnce {
    const NAME: &'static str = "components-return-once";
    const CATEGORY: RuleCategory = RuleCategory::Correctness;
}

impl ComponentsReturnOnce {
    pub fn new() -> Self {
        Self
    }

    /// Check a function for early returns and conditional returns
    pub fn check_function<'a>(
        &self,
        func: &Function<'a>,
        is_component: bool,
        is_render_prop: bool,
    ) -> Vec<Diagnostic> {
        if !is_component || is_render_prop {
            return Vec::new();
        }

        // Check if function name starts with lowercase (not a component)
        if let Some(id) = &func.id {
            if id.name.chars().next().is_some_and(|c| c.is_lowercase()) {
                return Vec::new();
            }
        }

        if let Some(body) = &func.body {
            self.check_body(body)
        } else {
            Vec::new()
        }
    }

    /// Check an arrow function for early returns and conditional returns
    pub fn check_arrow<'a>(
        &self,
        arrow: &ArrowFunctionExpression<'a>,
        is_component: bool,
        is_render_prop: bool,
    ) -> Vec<Diagnostic> {
        if !is_component || is_render_prop {
            return Vec::new();
        }

        self.check_body(&arrow.body)
    }

    fn check_body(&self, body: &FunctionBody) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let statements = &body.statements;

        if statements.is_empty() {
            return diagnostics;
        }

        // Find the last non-declaration statement (the "main" return)
        let last_return_idx = statements.iter().rposition(|stmt| {
            !matches!(
                stmt,
                Statement::FunctionDeclaration(_)
                    | Statement::ClassDeclaration(_)
                    | Statement::VariableDeclaration(_)
            )
        });

        // Check for early returns (any return that isn't the last statement)
        for (idx, stmt) in statements.iter().enumerate() {
            if Some(idx) != last_return_idx {
                if let Statement::ReturnStatement(ret) = stmt {
                    diagnostics.push(
                        Diagnostic::warning(
                            Self::NAME,
                            ret.span,
                            "Solid components run once, so an early return breaks reactivity. Move the condition inside a JSX element, such as a fragment or <Show />.",
                        ),
                    );
                }
            }
        }

        // Check if the last statement is a conditional return
        if let Some(idx) = last_return_idx {
            if let Statement::ReturnStatement(ret) = &statements[idx] {
                if let Some(arg) = &ret.argument {
                    match arg {
                        Expression::ConditionalExpression(cond) => {
                            diagnostics.push(
                                Diagnostic::warning(
                                    Self::NAME,
                                    cond.span,
                                    "Solid components run once, so a conditional return breaks reactivity. Move the condition inside a JSX element, such as a fragment or <Show />.",
                                )
                                .with_help("Use <Show when={condition}> or <Switch><Match when={condition}> instead."),
                            );
                        }
                        Expression::LogicalExpression(logical) => {
                            if matches!(
                                logical.operator,
                                oxc_syntax::operator::LogicalOperator::And
                            ) {
                                diagnostics.push(
                                    Diagnostic::warning(
                                        Self::NAME,
                                        logical.span,
                                        "Solid components run once, so a conditional return breaks reactivity. Move the condition inside a JSX element, such as a fragment or <Show />.",
                                    )
                                    .with_help("Use <Show when={condition}> instead."),
                                );
                            }
                        }
                        _ => {}
                    }
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
        assert_eq!(ComponentsReturnOnce::NAME, "components-return-once");
    }
}
