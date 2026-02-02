//! solid/no-proxy-apis
//!
//! Disallow usage of APIs that use ES6 Proxies, for environments that don't support them.

use oxc_ast::ast::{
    Argument, CallExpression, Expression, ImportDeclaration, JSXSpreadAttribute,
    NewExpression,
};
use oxc_span::GetSpan;

use crate::diagnostic::Diagnostic;
use crate::{RuleCategory, RuleMeta};

/// no-proxy-apis rule
#[derive(Debug, Clone, Default)]
pub struct NoProxyApis;

impl RuleMeta for NoProxyApis {
    const NAME: &'static str = "no-proxy-apis";
    const CATEGORY: RuleCategory = RuleCategory::Correctness;
}

impl NoProxyApis {
    pub fn new() -> Self {
        Self
    }

    /// Check an import declaration for solid-js/store
    pub fn check_import<'a>(&self, import: &ImportDeclaration<'a>) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        if import.source.value == "solid-js/store" {
            diagnostics.push(Diagnostic::warning(
                Self::NAME,
                import.span,
                "Solid Store APIs use Proxies, which are incompatible with your target environment.",
            ));
        }

        diagnostics
    }

    /// Check a JSX spread attribute for proxy-creating patterns
    pub fn check_spread<'a>(&self, spread: &JSXSpreadAttribute<'a>) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        // Check if expression is a member expression or call expression
        if spread.argument.is_member_expression() {
            diagnostics.push(Diagnostic::warning(
                Self::NAME,
                spread.span,
                "Using a property access in JSX spread makes Solid use Proxies, which are incompatible with your target environment.",
            ));
        } else if matches!(&spread.argument, Expression::CallExpression(_)) {
            diagnostics.push(Diagnostic::warning(
                Self::NAME,
                spread.span,
                "Using a function call in JSX spread makes Solid use Proxies, which are incompatible with your target environment.",
            ));
        }

        diagnostics
    }

    /// Check a new expression for `new Proxy()`
    pub fn check_new_expression<'a>(&self, new_expr: &NewExpression<'a>) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        if let Expression::Identifier(ident) = &new_expr.callee {
            if ident.name == "Proxy" {
                diagnostics.push(Diagnostic::warning(
                    Self::NAME,
                    new_expr.span,
                    "Proxies are incompatible with your target environment.",
                ));
            }
        }

        diagnostics
    }

    /// Check a call expression for Proxy.revocable() and mergeProps with functions
    pub fn check_call<'a>(&self, call: &CallExpression<'a>) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        // Check for Proxy.revocable()
        if let Expression::StaticMemberExpression(member) = &call.callee {
            if let Expression::Identifier(obj) = &member.object {
                if obj.name == "Proxy" && member.property.name == "revocable" {
                    diagnostics.push(Diagnostic::warning(
                        Self::NAME,
                        call.span,
                        "Proxies are incompatible with your target environment.",
                    ));
                }
            }
        }

        // Check for mergeProps with function/variable arguments
        if let Expression::Identifier(callee) = &call.callee {
            if callee.name == "mergeProps" {
                for arg in &call.arguments {
                    let is_problematic = match arg {
                        Argument::SpreadElement(_) => true,
                        arg => {
                            if let Some(expr) = arg.as_expression() {
                                matches!(
                                    expr,
                                    Expression::Identifier(_)
                                        | Expression::ArrowFunctionExpression(_)
                                        | Expression::FunctionExpression(_)
                                )
                            } else {
                                false
                            }
                        }
                    };

                    if is_problematic {
                        diagnostics.push(Diagnostic::warning(
                            Self::NAME,
                            arg.span(),
                            "If you pass a function to `mergeProps`, it will create a Proxy, which is incompatible with your target environment.",
                        ));
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
        assert_eq!(NoProxyApis::NAME, "no-proxy-apis");
    }
}
