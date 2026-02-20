//! Semantic-aware lint visitor for Phase 2 rules
//!
//! This module provides a `SemanticLintRunner` that integrates with oxc_semantic
//! for proper scope resolution and symbol tracking.

use oxc_ast::ast::{
    Argument, ArrowFunctionExpression, CallExpression, Expression, Function,
    ImportDeclaration, ImportDeclarationSpecifier, JSXElementName, JSXMemberExpressionObject,
    JSXOpeningElement, Program, Statement,
};
use oxc_ast_visit::{walk, Visit};
use oxc_semantic::{ScopeId, Semantic, SymbolId};
use oxc_span::{GetSpan, SourceType, Span};
use rustc_hash::FxHashSet;

use crate::diagnostic::Diagnostic;
use crate::rules::jsx_no_undef::JsxNoUndef;
use crate::rules::{ComponentsReturnOnce, NoDestructure, Reactivity};
use crate::utils::is_dom_element;
use crate::RuleMeta;

/// Solid.js module sources
const SOLID_SOURCES: &[&str] = &["solid-js", "solid-js/store", "solid-js/web"];

/// Configuration for semantic-aware rules
#[derive(Debug, Clone, Default)]
pub struct SemanticRulesConfig {
    pub jsx_no_undef: bool,
    pub jsx_uses_vars: bool,
    pub components_return_once: bool,
    pub reactivity: bool,
    pub no_destructure: bool,
}

impl SemanticRulesConfig {
    pub fn all() -> Self {
        Self {
            jsx_no_undef: true,
            jsx_uses_vars: true,
            components_return_once: true,
            reactivity: true,
            no_destructure: true,
        }
    }

    pub fn none() -> Self {
        Self::default()
    }
}

/// Result of semantic linting
#[derive(Debug)]
pub struct SemanticLintResult {
    pub diagnostics: Vec<Diagnostic>,
    pub used_symbols: FxHashSet<SymbolId>,
    pub component_symbols: FxHashSet<SymbolId>,
}

/// Semantic-aware lint runner that uses oxc_semantic for scope resolution
#[allow(dead_code)]
pub struct SemanticLintRunner<'a> {
    semantic: &'a Semantic<'a>,
    source_text: &'a str,
    source_type: SourceType,
    config: SemanticRulesConfig,
    diagnostics: Vec<Diagnostic>,
    /// Symbols marked as used (for jsx-uses-vars)
    used_symbols: FxHashSet<SymbolId>,
    /// Symbols identified as components
    component_symbols: FxHashSet<SymbolId>,
    /// Solid imports (function names imported from solid-js)
    solid_imports: FxHashSet<String>,
    /// Current scope stack for tracking nested scopes
    scope_stack: Vec<ScopeId>,
    /// Functions that contain JSX (potential components)
    functions_with_jsx: FxHashSet<Span>,
    /// Track if we're inside a JSX expression
    jsx_depth: usize,
}

impl<'a> SemanticLintRunner<'a> {
    pub fn new(
        semantic: &'a Semantic<'a>,
        source_text: &'a str,
        source_type: SourceType,
        config: SemanticRulesConfig,
    ) -> Self {
        Self {
            semantic,
            source_text,
            source_type,
            config,
            diagnostics: Vec::new(),
            used_symbols: FxHashSet::default(),
            component_symbols: FxHashSet::default(),
            solid_imports: FxHashSet::default(),
            scope_stack: vec![semantic.scoping().root_scope_id()],
            functions_with_jsx: FxHashSet::default(),
            jsx_depth: 0,
        }
    }

    /// Run the semantic linter on the program
    pub fn run(mut self, program: &Program<'a>) -> SemanticLintResult {
        // Collect imports from solid-js
        self.collect_solid_imports(program);

        // Visit AST and run rules
        self.visit_program(program);

        SemanticLintResult {
            diagnostics: self.diagnostics,
            used_symbols: self.used_symbols,
            component_symbols: self.component_symbols,
        }
    }

    /// Get the current scope ID
    fn current_scope(&self) -> ScopeId {
        *self.scope_stack.last().unwrap_or(&self.semantic.scoping().root_scope_id())
    }

    /// Resolve an identifier name in the current scope
    #[allow(dead_code)]
    fn resolve_in_current_scope(&self, name: &str) -> Option<SymbolId> {
        self.semantic
            .scoping()
            .find_binding(self.current_scope(), name.into())
    }

    /// Check if we're inside a JSX expression context
    fn is_inside_jsx(&self) -> bool {
        self.jsx_depth > 0
    }

    // ==================== Phase 1: Import and Type Inference ====================

    /// Collect imports from solid-js modules
    fn collect_solid_imports(&mut self, program: &Program<'a>) {
        for stmt in &program.body {
            if let Statement::ImportDeclaration(import) = stmt {
                self.process_import(import);
            }
        }
    }

    fn process_import(&mut self, import: &ImportDeclaration<'a>) {
        let source = import.source.value.as_str();
        if !SOLID_SOURCES.iter().any(|s| source.starts_with(s)) {
            return;
        }

        if let Some(specifiers) = &import.specifiers {
            for spec in specifiers {
                match spec {
                    ImportDeclarationSpecifier::ImportSpecifier(named) => {
                        let local_name = named.local.name.as_str();
                        self.solid_imports.insert(local_name.to_string());
                    }
                    ImportDeclarationSpecifier::ImportDefaultSpecifier(default) => {
                        self.solid_imports.insert(default.local.name.to_string());
                    }
                    ImportDeclarationSpecifier::ImportNamespaceSpecifier(ns) => {
                        self.solid_imports.insert(ns.local.name.to_string());
                    }
                }
            }
        }
    }

    // ==================== Phase 2: JSX Rules ====================

    /// Check JSX opening element for jsx-no-undef and jsx-uses-vars
    fn check_jsx_opening_element(&mut self, opening: &JSXOpeningElement<'a>) {
        let scope_id = self.current_scope();

        // Extract the identifier name and check if it's a component
        match &opening.name {
            JSXElementName::Identifier(ident) => {
                let name = &ident.name;
                if !is_dom_element(name) && name.as_str() != "this" {
                    self.check_jsx_identifier(name, ident.span, scope_id, true);
                }
            }
            JSXElementName::IdentifierReference(ident) => {
                let name = &ident.name;
                if !is_dom_element(name) && name.as_str() != "this" {
                    self.check_jsx_identifier(name, ident.span, scope_id, true);
                }
            }
            JSXElementName::MemberExpression(member) => {
                // For <Foo.Bar>, check the root (Foo)
                if let Some((name, span)) = self.get_member_root(member) {
                    if name != "this" {
                        self.check_jsx_identifier(&name, span, scope_id, false);
                    }
                }
            }
            JSXElementName::NamespacedName(_) | JSXElementName::ThisExpression(_) => {}
        }

        // Check use:X custom directives
        for attr in &opening.attributes {
            if let oxc_ast::ast::JSXAttributeItem::Attribute(jsx_attr) = attr {
                if let oxc_ast::ast::JSXAttributeName::NamespacedName(ns) = &jsx_attr.name {
                    if ns.namespace.name == "use" {
                        let directive_name = ns.name.name.as_str();
                        self.check_jsx_identifier(
                            directive_name,
                            ns.name.span,
                            scope_id,
                            false,
                        );
                    }
                }
            }
        }
    }

    fn check_jsx_identifier(
        &mut self,
        name: &str,
        span: Span,
        scope_id: ScopeId,
        is_component: bool,
    ) {
        let scoping = self.semantic.scoping();
        let symbol_id = scoping.find_binding(scope_id, name.into());

        if let Some(symbol_id) = symbol_id {
            // jsx-uses-vars: mark as used
            if self.config.jsx_uses_vars {
                self.used_symbols.insert(symbol_id);
            }

            // If it's used as a component tag, mark it as a component
            if is_component {
                self.component_symbols.insert(symbol_id);
            }
        } else if self.config.jsx_no_undef {
            // Check if it's a Solid auto-import component
            let auto_components = ["Show", "For", "Index", "Switch", "Match"];
            if auto_components.contains(&name) {
                self.diagnostics.push(
                    Diagnostic::error(
                        JsxNoUndef::NAME,
                        span,
                        format!("'{}' should be imported from 'solid-js'.", name),
                    )
                    .with_help(format!("Add: import {{ {} }} from \"solid-js\";", name)),
                );
            } else {
                self.diagnostics.push(Diagnostic::error(
                    JsxNoUndef::NAME,
                    span,
                    format!("'{}' is not defined.", name),
                ));
            }
        }
    }

    fn get_member_root(
        &self,
        member: &oxc_ast::ast::JSXMemberExpression<'a>,
    ) -> Option<(String, Span)> {
        let mut current = &member.object;
        loop {
            match current {
                JSXMemberExpressionObject::IdentifierReference(ident) => {
                    return Some((ident.name.to_string(), ident.span));
                }
                JSXMemberExpressionObject::MemberExpression(inner) => {
                    current = &inner.object;
                }
                JSXMemberExpressionObject::ThisExpression(_) => {
                    return None;
                }
            }
        }
    }

    // ==================== Phase 2: Component Detection ====================

    /// Check if a function is a component and run components-return-once
    fn check_function_component(&mut self, func: &Function<'a>) {
        if !self.config.components_return_once {
            return;
        }

        // Skip if inside JSX expression (render props, callbacks)
        if self.is_inside_jsx() {
            return;
        }

        // Heuristic 1: PascalCase name
        let is_pascal_case = func.id.as_ref().is_some_and(|id| {
            id.name.chars().next().is_some_and(|c| c.is_uppercase())
        });

        // Heuristic 2: Returns JSX
        let returns_jsx = func.body.as_ref().is_some_and(|body| {
            NoDestructure::body_has_jsx(body)
        });

        if !is_pascal_case && !returns_jsx {
            return;
        }

        // If we have a symbol ID, check if it's marked as a component
        let is_known_component = func.id.as_ref().map(|id| {
            let sym = id.symbol_id();
            self.component_symbols.contains(&sym)
        }).unwrap_or(false);

        if is_pascal_case || returns_jsx || is_known_component {
            let rule = ComponentsReturnOnce::new();
            if func.body.is_some() {
                self.diagnostics.extend(
                    rule.check_function(func, true, self.is_inside_jsx())
                );
            }
        }
    }

    fn check_arrow_component(&mut self, arrow: &ArrowFunctionExpression<'a>) {
        if !self.config.components_return_once && !self.config.no_destructure {
            return;
        }

        // Skip if inside JSX expression
        if self.is_inside_jsx() {
            return;
        }

        // Check if returns JSX
        let returns_jsx = NoDestructure::body_has_jsx(&arrow.body);
        if !returns_jsx {
            return;
        }

        if self.config.components_return_once {
            let rule = ComponentsReturnOnce::new();
            self.diagnostics.extend(
                rule.check_arrow(arrow, true, self.is_inside_jsx())
            );
        }

        if self.config.no_destructure {
            let rule = NoDestructure::new();
            self.diagnostics.extend(
                rule.check_arrow(arrow, returns_jsx, self.is_inside_jsx())
            );
        }
    }

    // ==================== Phase 3: Reactivity Checks ====================

    fn check_call_expression(&mut self, call: &CallExpression<'a>) {
        if !self.config.reactivity {
            return;
        }

        // Check for signal getter called without parens (accessing as property)
        // This is a common mistake: signal.value instead of signal().value

        // Check for reactive primitives receiving non-function arguments
        let callee_name = match &call.callee {
            Expression::Identifier(ident) => Some(ident.name.as_str()),
            _ => None,
        };

        let Some(callee_name) = callee_name else {
            return;
        };

        let reactive_primitives = [
            "createEffect",
            "createMemo",
            "createComputed",
            "createRenderEffect",
            "createReaction",
            "on",
        ];

        if reactive_primitives.contains(&callee_name) {
            if let Some(first_arg) = call.arguments.first() {
                match first_arg {
                    Argument::SpreadElement(_) => {}
                    arg => {
                        if let Some(expr) = arg.as_expression() {
                            if !matches!(
                                expr,
                                Expression::ArrowFunctionExpression(_)
                                    | Expression::FunctionExpression(_)
                                    | Expression::Identifier(_)
                            ) {
                                self.diagnostics.push(Diagnostic::warning(
                                    Reactivity::NAME,
                                    expr.span(),
                                    format!(
                                        "`{}` expects a function. Passing a non-function value may cause reactivity issues.",
                                        callee_name
                                    ),
                                ));
                            }
                        }
                    }
                }
            }
        }
    }
}

impl<'a> Visit<'a> for SemanticLintRunner<'a> {
    fn visit_program(&mut self, program: &Program<'a>) {
        walk::walk_program(self, program);
    }

    fn visit_function(&mut self, func: &Function<'a>, _flags: oxc_syntax::scope::ScopeFlags) {
        // Check function as component
        self.check_function_component(func);

        // Check for destructured props
        if self.config.no_destructure && !self.is_inside_jsx() {
            let returns_jsx = func.body.as_ref().is_some_and(|b| NoDestructure::body_has_jsx(b));
            if returns_jsx {
                let rule = NoDestructure::new();
                self.diagnostics.extend(
                    rule.check_function(func, returns_jsx, self.is_inside_jsx())
                );
            }
        }

        // Push new scope (simplified - in full impl would track actual scope IDs)
        walk::walk_function(self, func, _flags);
    }

    fn visit_arrow_function_expression(&mut self, arrow: &ArrowFunctionExpression<'a>) {
        self.check_arrow_component(arrow);
        walk::walk_arrow_function_expression(self, arrow);
    }

    fn visit_jsx_opening_element(&mut self, opening: &JSXOpeningElement<'a>) {
        self.check_jsx_opening_element(opening);
        walk::walk_jsx_opening_element(self, opening);
    }

    fn visit_jsx_element(&mut self, element: &oxc_ast::ast::JSXElement<'a>) {
        self.jsx_depth += 1;
        walk::walk_jsx_element(self, element);
        self.jsx_depth -= 1;
    }

    fn visit_jsx_fragment(&mut self, fragment: &oxc_ast::ast::JSXFragment<'a>) {
        self.jsx_depth += 1;
        walk::walk_jsx_fragment(self, fragment);
        self.jsx_depth -= 1;
    }

    fn visit_call_expression(&mut self, call: &CallExpression<'a>) {
        self.check_call_expression(call);
        walk::walk_call_expression(self, call);
    }
}

/// Convenience function to run semantic linting
pub fn lint_with_semantic<'a>(
    semantic: &'a Semantic<'a>,
    source_text: &'a str,
    source_type: SourceType,
    program: &Program<'a>,
) -> SemanticLintResult {
    let config = SemanticRulesConfig::all();
    SemanticLintRunner::new(semantic, source_text, source_type, config).run(program)
}

/// Convenience function to run semantic linting with custom config
pub fn lint_with_semantic_config<'a>(
    semantic: &'a Semantic<'a>,
    source_text: &'a str,
    source_type: SourceType,
    program: &Program<'a>,
    config: SemanticRulesConfig,
) -> SemanticLintResult {
    SemanticLintRunner::new(semantic, source_text, source_type, config).run(program)
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxc_allocator::Allocator;
    use oxc_parser::Parser;
    use oxc_semantic::SemanticBuilder;

    fn parse_and_lint(source: &str) -> SemanticLintResult {
        let allocator = Allocator::default();
        let source_type = SourceType::jsx();
        let ret = Parser::new(&allocator, source, source_type).parse();

        let semantic_ret = SemanticBuilder::new()
            .with_excess_capacity(0.0)
            .build(&ret.program);

        lint_with_semantic(&semantic_ret.semantic, source, source_type, &ret.program)
    }

    #[test]
    fn test_jsx_uses_vars() {
        let result = parse_and_lint(
            r#"
            import { Show } from 'solid-js';
            function App() {
                return <Show when={true}>hello</Show>;
            }
            "#,
        );
        assert!(!result.used_symbols.is_empty());
    }

    #[test]
    fn test_jsx_no_undef() {
        let result = parse_and_lint(
            r#"
            function App() {
                return <UndefinedComponent />;
            }
            "#,
        );
        assert!(result.diagnostics.iter().any(|d| d.message.contains("not defined")));
    }

    #[test]
    fn test_auto_import_suggestion() {
        let result = parse_and_lint(
            r#"
            function App() {
                return <Show when={true}>hello</Show>;
            }
            "#,
        );
        assert!(result.diagnostics.iter().any(|d| d.message.contains("solid-js")));
    }

    #[test]
    fn test_component_detection() {
        let result = parse_and_lint(
            r#"
            function Button() {
                return <button>Click me</button>;
            }
            function App() {
                return <Button />;
            }
            "#,
        );
        // Button should be marked as a component because it's used in JSX
        assert!(!result.component_symbols.is_empty());
    }

    #[test]
    fn test_jsx_member_expression() {
        let result = parse_and_lint(
            r#"
            const Icons = { Star: () => <svg /> };
            function App() {
                return <Icons.Star />;
            }
            "#,
        );
        // Icons should be marked as used
        assert!(!result.used_symbols.is_empty());
    }

    #[test]
    fn test_custom_directive_undefined() {
        let result = parse_and_lint(
            r#"
            function App() {
                return <div use:undefinedDirective />;
            }
            "#,
        );
        assert!(result.diagnostics.iter().any(|d| d.message.contains("not defined")));
    }

    #[test]
    fn test_custom_directive_defined() {
        let result = parse_and_lint(
            r#"
            function myDirective(el) { el.focus(); }
            function App() {
                return <input use:myDirective />;
            }
            "#,
        );
        // Should not report undefined for myDirective
        assert!(!result.diagnostics.iter().any(|d| 
            d.message.contains("myDirective") && d.message.contains("not defined")
        ));
    }

    #[test]
    fn test_dom_elements_not_flagged() {
        let result = parse_and_lint(
            r#"
            function App() {
                return <div><span>text</span></div>;
            }
            "#,
        );
        // DOM elements should not be flagged as undefined
        assert!(!result.diagnostics.iter().any(|d| d.message.contains("div")));
        assert!(!result.diagnostics.iter().any(|d| d.message.contains("span")));
    }

    #[test]
    fn test_multiple_solid_control_flow() {
        let result = parse_and_lint(
            r#"
            function App() {
                return (
                    <Show when={true}>
                        <For each={items}>{(item) => <div>{item}</div>}</For>
                    </Show>
                );
            }
            "#,
        );
        // Should suggest importing Show and For
        let has_import_suggestion = result.diagnostics.iter().any(|d| 
            d.message.contains("solid-js") && 
            (d.message.contains("Show") || d.message.contains("For"))
        );
        assert!(has_import_suggestion);
    }

    #[test]
    fn test_reactivity_non_function_arg() {
        let result = parse_and_lint(
            r#"
            import { createEffect } from 'solid-js';
            createEffect(5 + 3);
            "#,
        );
        assert!(result.diagnostics.iter().any(|d| 
            d.message.contains("createEffect") && d.message.contains("function")
        ));
    }

    #[test]
    fn test_reactivity_valid_function_arg() {
        let result = parse_and_lint(
            r#"
            import { createEffect } from 'solid-js';
            createEffect(() => console.log('effect'));
            "#,
        );
        // Should not warn about function argument
        assert!(!result.diagnostics.iter().any(|d| 
            d.message.contains("createEffect") && d.message.contains("function")
        ));
    }

    #[test]
    fn test_solid_imports_tracked() {
        let result = parse_and_lint(
            r#"
            import { createSignal, createEffect, Show } from 'solid-js';
            import { createStore } from 'solid-js/store';
            "#,
        );
        // No diagnostics expected for just imports
        assert!(result.diagnostics.is_empty());
    }
}
