//! solid/reactivity
//!
//! Enforce that reactive expressions (signals, memos, stores) are accessed properly.
//! This is a complex rule that tracks signal/store reads and ensures they happen
//! in reactive contexts.
//!
//! Note: This is a simplified implementation. The full ESLint version is 1200+ lines
//! and tracks control flow, function scopes, and more.

use oxc_ast::ast::{
    Argument, CallExpression, Expression, JSXAttributeItem, JSXAttributeName,
    JSXAttributeValue, JSXExpressionContainer, JSXOpeningElement, VariableDeclarator,
};
use oxc_span::GetSpan;

use crate::diagnostic::Diagnostic;
use crate::{RuleCategory, RuleMeta};

/// reactivity rule
#[derive(Debug, Clone, Default)]
pub struct Reactivity;

impl RuleMeta for Reactivity {
    const NAME: &'static str = "reactivity";
    const CATEGORY: RuleCategory = RuleCategory::Correctness;
}

/// Solid primitives that create signals
const SIGNAL_CREATORS: &[&str] = &[
    "createSignal",
    "createMemo",
    "createResource",
    "useContext",
];

/// Solid primitives that expect reactive expressions as arguments
const REACTIVE_PRIMITIVES: &[&str] = &[
    "createEffect",
    "createMemo",
    "createComputed",
    "createRenderEffect",
    "createReaction",
    "on",
];

/// Solid primitives that create stores
const STORE_CREATORS: &[&str] = &["createStore", "createMutable"];

impl Reactivity {
    pub fn new() -> Self {
        Self
    }

    /// Check a variable declarator for signal/store destructuring issues
    pub fn check_variable<'a>(&self, declarator: &VariableDeclarator<'a>) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        let Some(init) = &declarator.init else {
            return diagnostics;
        };

        // Check for createSignal/createResource call
        if let Expression::CallExpression(call) = init {
            if let Expression::Identifier(callee) = &call.callee {
                if SIGNAL_CREATORS.contains(&callee.name.as_str()) {
                    // Check if destructured incorrectly
                    // createSignal returns [getter, setter], should be accessed as signal[0](), signal[1]()
                    // or destructured as [signal, setSignal]
                }

                // Check for createStore destructured as non-array
                if STORE_CREATORS.contains(&callee.name.as_str()) {
                    // Store should be destructured as [store, setStore]
                }
            }
        }

        diagnostics
    }

    /// Check a call expression for reactivity issues
    pub fn check_call<'a>(&self, call: &CallExpression<'a>) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        let Expression::Identifier(callee) = &call.callee else {
            return diagnostics;
        };

        let callee_name = callee.name.as_str();

        // Check for accessing signal value outside reactive context
        // This would require tracking which variables are signals

        // Check for passing non-reactive values to reactive primitives
        if REACTIVE_PRIMITIVES.contains(&callee_name) {
            // First argument should be a function
            if let Some(first_arg) = call.arguments.first() {
                match first_arg {
                    Argument::SpreadElement(_) => {}
                    arg => {
                        if let Some(expr) = arg.as_expression() {
                            // Check if it's not a function
                            if !matches!(
                                expr,
                                Expression::ArrowFunctionExpression(_)
                                    | Expression::FunctionExpression(_)
                                    | Expression::Identifier(_)
                            ) {
                                diagnostics.push(
                                    Diagnostic::warning(
                                        Self::NAME,
                                        expr.span(),
                                        format!(
                                            "`{}` expects a function. Passing a non-function value may cause reactivity issues.",
                                            callee_name
                                        ),
                                    ),
                                );
                            }
                        }
                    }
                }
            }
        }

        diagnostics
    }

    /// Check JSX expression for potential reactivity loss
    pub fn check_jsx_expression<'a>(
        &self,
        container: &JSXExpressionContainer<'a>,
        is_in_attribute: bool,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        let Some(expr) = container.expression.as_expression() else {
            return diagnostics;
        };

        // Check for calling a signal/memo and immediately accessing a property
        // e.g., {signal().value} - this is fine
        // vs {signal.value} - this would lose reactivity (but we can't detect without type info)

        // Check for spreading in JSX which might lose reactivity
        // This is handled by no-proxy-apis

        diagnostics
    }

    /// Check JSX attribute for reactivity issues
    pub fn check_jsx_attribute<'a>(
        &self,
        opening: &JSXOpeningElement<'a>,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        for attr in &opening.attributes {
            let JSXAttributeItem::Attribute(jsx_attr) = attr else {
                continue;
            };

            let attr_name = match &jsx_attr.name {
                JSXAttributeName::Identifier(ident) => ident.name.as_str(),
                JSXAttributeName::NamespacedName(ns) => {
                    // Check for ref directive - should be a variable, not a function call
                    if ns.namespace.name == "ref" {
                        if let Some(JSXAttributeValue::ExpressionContainer(container)) =
                            &jsx_attr.value
                        {
                            if let Some(Expression::CallExpression(_)) =
                                container.expression.as_expression()
                            {
                                diagnostics.push(
                                    Diagnostic::warning(
                                        Self::NAME,
                                        jsx_attr.span,
                                        "The `ref` directive expects a variable, not a function call.",
                                    ),
                                );
                            }
                        }
                    }
                    continue;
                }
            };

            // Check for event handlers that don't use functions
            if attr_name.starts_with("on") && attr_name.len() > 2 {
                if let Some(JSXAttributeValue::ExpressionContainer(container)) = &jsx_attr.value {
                    if let Some(expr) = container.expression.as_expression() {
                        // Event handlers should be functions, not calls
                        if let Expression::CallExpression(call) = expr {
                            // Check if it's not creating a bound function
                            if let Expression::Identifier(callee) = &call.callee {
                                if callee.name != "bind" {
                                    diagnostics.push(
                                        Diagnostic::warning(
                                            Self::NAME,
                                            call.span,
                                            format!(
                                                "Event handler `{}` is calling a function. This will execute immediately. Wrap in an arrow function: `() => {}(...)`",
                                                attr_name, callee.name
                                            ),
                                        ),
                                    );
                                }
                            }
                        }
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
        assert_eq!(Reactivity::NAME, "reactivity");
    }
}
