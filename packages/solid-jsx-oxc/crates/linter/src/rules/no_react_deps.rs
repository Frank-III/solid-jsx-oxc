//! solid/no-react-deps
//!
//! Disallow usage of dependency arrays in createEffect and createMemo.

use oxc_ast::ast::{Argument, CallExpression, Expression};
use oxc_span::GetSpan;

use crate::diagnostic::{Diagnostic, Fix};
use crate::{RuleCategory, RuleMeta};

#[derive(Debug, Clone, Default)]
pub struct NoReactDeps;

impl RuleMeta for NoReactDeps {
    const NAME: &'static str = "no-react-deps";
    const CATEGORY: RuleCategory = RuleCategory::Correctness;
}

impl NoReactDeps {
    pub fn new() -> Self {
        Self
    }

    pub fn check<'a>(&self, call: &CallExpression<'a>) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        let callee_name = match &call.callee {
            Expression::Identifier(ident) => &ident.name,
            _ => return diagnostics,
        };

        if callee_name != "createEffect" && callee_name != "createMemo" {
            return diagnostics;
        }

        if call.arguments.len() != 2 {
            return diagnostics;
        }

        if call.arguments.iter().any(|arg| matches!(arg, Argument::SpreadElement(_))) {
            return diagnostics;
        }

        let Some(first_arg) = call.arguments.first() else {
            return diagnostics;
        };
        let Some(second_arg) = call.arguments.get(1) else {
            return diagnostics;
        };

        let first_expr = match first_arg {
            Argument::SpreadElement(_) => return diagnostics,
            arg => arg.to_expression(),
        };

        let second_expr = match second_arg {
            Argument::SpreadElement(_) => return diagnostics,
            arg => arg.to_expression(),
        };

        let is_zero_param_function = match first_expr {
            Expression::FunctionExpression(func) => func.params.items.is_empty(),
            Expression::ArrowFunctionExpression(arrow) => arrow.params.items.is_empty(),
            _ => false,
        };

        if !is_zero_param_function {
            return diagnostics;
        }

        let is_array_expression = matches!(second_expr, Expression::ArrayExpression(_));

        if !is_array_expression {
            return diagnostics;
        }

        let second_span = second_arg.span();

        let mut diagnostic = Diagnostic::warning(
            Self::NAME,
            second_span,
            format!(
                "In Solid, `{}` doesn't accept a dependency array because it automatically tracks its dependencies. If you really need to override the list of dependencies, use `on`.",
                callee_name
            ),
        );

        let fix_start = call.arguments.first().unwrap().span().end;
        let fix_end = second_span.end;
        let fix_span = oxc_span::Span::new(fix_start, fix_end);
        diagnostic = diagnostic.with_fix(
            Fix::new(fix_span, String::new()).with_message("Remove dependency array"),
        );

        diagnostics.push(diagnostic);
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rule_name() {
        assert_eq!(NoReactDeps::NAME, "no-react-deps");
    }
}
