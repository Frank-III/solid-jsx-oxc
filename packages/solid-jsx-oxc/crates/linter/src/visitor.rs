//! Unified visitor pattern for running all lint rules in a single AST pass
//!
//! This module provides a `LintRunner` that traverses the AST once and runs
//! all enabled rules during the traversal, collecting diagnostics efficiently.

use oxc_ast::ast::{JSXElement, JSXFragment, JSXOpeningElement, Program};
use oxc_ast_visit::{walk, Visit};
use oxc_semantic::Semantic;
use oxc_span::SourceType;

use crate::diagnostic::Diagnostic;
use crate::rules::{
    JsxNoDuplicateProps, JsxNoScriptUrl, JsxUsesVars, NoInnerhtml, NoReactSpecificProps,
    NoUnknownNamespaces, PreferClasslist, PreferFor, PreferShow, SelfClosingComp, StyleProp,
};

/// Configuration for which rules are enabled
#[derive(Debug, Clone)]
pub struct RulesConfig {
    pub jsx_no_duplicate_props: Option<JsxNoDuplicateProps>,
    pub jsx_no_script_url: Option<JsxNoScriptUrl>,
    pub jsx_uses_vars: bool,
    pub no_innerhtml: Option<NoInnerhtml>,
    pub no_react_specific_props: bool,
    pub no_unknown_namespaces: Option<NoUnknownNamespaces>,
    pub prefer_classlist: bool,
    pub prefer_for: bool,
    pub prefer_show: bool,
    pub self_closing_comp: Option<SelfClosingComp>,
    pub style_prop: Option<StyleProp>,
}

impl Default for RulesConfig {
    fn default() -> Self {
        Self {
            jsx_no_duplicate_props: Some(JsxNoDuplicateProps::new()),
            jsx_no_script_url: Some(JsxNoScriptUrl::new()),
            jsx_uses_vars: true,
            no_innerhtml: Some(NoInnerhtml::new()),
            no_react_specific_props: true,
            no_unknown_namespaces: Some(NoUnknownNamespaces::new()),
            prefer_classlist: true,
            prefer_for: true,
            prefer_show: true,
            self_closing_comp: Some(SelfClosingComp::new()),
            style_prop: Some(StyleProp::new()),
        }
    }
}

impl RulesConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn none() -> Self {
        Self {
            jsx_no_duplicate_props: None,
            jsx_no_script_url: None,
            jsx_uses_vars: false,
            no_innerhtml: None,
            no_react_specific_props: false,
            no_unknown_namespaces: None,
            prefer_classlist: false,
            prefer_for: false,
            prefer_show: false,
            self_closing_comp: None,
            style_prop: None,
        }
    }

    pub fn with_jsx_no_duplicate_props(mut self, rule: JsxNoDuplicateProps) -> Self {
        self.jsx_no_duplicate_props = Some(rule);
        self
    }

    pub fn with_jsx_no_script_url(mut self, rule: JsxNoScriptUrl) -> Self {
        self.jsx_no_script_url = Some(rule);
        self
    }

    pub fn with_jsx_uses_vars(mut self, enabled: bool) -> Self {
        self.jsx_uses_vars = enabled;
        self
    }

    pub fn with_no_innerhtml(mut self, rule: NoInnerhtml) -> Self {
        self.no_innerhtml = Some(rule);
        self
    }

    pub fn with_no_react_specific_props(mut self, enabled: bool) -> Self {
        self.no_react_specific_props = enabled;
        self
    }

    pub fn with_no_unknown_namespaces(mut self, rule: NoUnknownNamespaces) -> Self {
        self.no_unknown_namespaces = Some(rule);
        self
    }

    pub fn with_prefer_classlist(mut self, enabled: bool) -> Self {
        self.prefer_classlist = enabled;
        self
    }

    pub fn with_prefer_for(mut self, enabled: bool) -> Self {
        self.prefer_for = enabled;
        self
    }

    pub fn with_prefer_show(mut self, enabled: bool) -> Self {
        self.prefer_show = enabled;
        self
    }

    pub fn with_self_closing_comp(mut self, rule: SelfClosingComp) -> Self {
        self.self_closing_comp = Some(rule);
        self
    }

    pub fn with_style_prop(mut self, rule: StyleProp) -> Self {
        self.style_prop = Some(rule);
        self
    }
}

/// Context for lint execution
pub struct VisitorLintContext<'a> {
    source_text: &'a str,
    source_type: SourceType,
    semantic: Option<&'a Semantic<'a>>,
}

impl<'a> VisitorLintContext<'a> {
    pub fn new(source_text: &'a str, source_type: SourceType) -> Self {
        Self {
            source_text,
            source_type,
            semantic: None,
        }
    }

    pub fn with_semantic(mut self, semantic: &'a Semantic<'a>) -> Self {
        self.semantic = Some(semantic);
        self
    }

    pub fn source_text(&self) -> &'a str {
        self.source_text
    }

    pub fn source_type(&self) -> SourceType {
        self.source_type
    }

    pub fn semantic(&self) -> Option<&'a Semantic<'a>> {
        self.semantic
    }
}

/// Unified visitor that runs all enabled rules during a single AST traversal
pub struct LintRunner<'a> {
    ctx: VisitorLintContext<'a>,
    config: RulesConfig,
    diagnostics: Vec<Diagnostic>,
    used_vars: Vec<String>,
}

impl<'a> LintRunner<'a> {
    pub fn new(ctx: VisitorLintContext<'a>, config: RulesConfig) -> Self {
        Self {
            ctx,
            config,
            diagnostics: Vec::new(),
            used_vars: Vec::new(),
        }
    }

    /// Run all enabled rules on the given program
    pub fn run(mut self, program: &Program<'a>) -> LintResult {
        self.visit_program(program);
        LintResult {
            diagnostics: self.diagnostics,
            used_vars: self.used_vars,
        }
    }

    /// Check a JSX element with all applicable rules
    fn check_jsx_element(&mut self, element: &JSXElement<'a>) {
        let opening = &element.opening_element;
        let children = &element.children;
        let closing_span = element.closing_element.as_ref().map(|c| c.span);

        // jsx-no-duplicate-props
        if let Some(rule) = &self.config.jsx_no_duplicate_props {
            self.diagnostics.extend(rule.check(opening, children));
        }

        // no-innerhtml (needs full element for children check)
        if let Some(rule) = &self.config.no_innerhtml {
            self.diagnostics.extend(rule.check(element));
        }

        // self-closing-comp
        if let Some(rule) = &self.config.self_closing_comp {
            self.diagnostics
                .extend(rule.check(opening, children, closing_span));
        }

        // prefer-for: check children for map() calls
        if self.config.prefer_for {
            let rule = PreferFor::new();
            self.diagnostics.extend(rule.check_element_children(element));
        }

        // prefer-show: check children for conditionals
        if self.config.prefer_show {
            let rule = PreferShow::new();
            self.diagnostics
                .extend(rule.check_element_children(element, self.ctx.source_text()));
        }
    }

    /// Check a JSX opening element with all applicable rules
    fn check_jsx_opening_element(&mut self, opening: &JSXOpeningElement<'a>) {
        // jsx-no-script-url
        if let Some(rule) = &self.config.jsx_no_script_url {
            self.diagnostics.extend(rule.check(opening));
        }

        // no-react-specific-props
        if self.config.no_react_specific_props {
            let rule = NoReactSpecificProps::new();
            self.diagnostics.extend(rule.check(opening));
        }

        // no-unknown-namespaces
        if let Some(rule) = &self.config.no_unknown_namespaces {
            self.diagnostics.extend(rule.check(opening));
        }

        // style-prop
        if let Some(rule) = &self.config.style_prop {
            self.diagnostics.extend(rule.check(opening));
        }

        // prefer-classlist
        if self.config.prefer_classlist {
            let rule = PreferClasslist::new();
            self.diagnostics.extend(rule.check(opening));
        }

        // jsx-uses-vars (collects used vars, doesn't produce diagnostics)
        if self.config.jsx_uses_vars {
            let rule = JsxUsesVars::new();
            self.used_vars.extend(rule.collect_used_vars(opening));
        }
    }

    /// Check a JSX fragment with applicable rules
    fn check_jsx_fragment(&mut self, fragment: &JSXFragment<'a>) {
        // prefer-for: check children for map() calls
        if self.config.prefer_for {
            let rule = PreferFor::new();
            self.diagnostics
                .extend(rule.check_fragment_children(fragment));
        }

        // prefer-show: check children for conditionals
        if self.config.prefer_show {
            let rule = PreferShow::new();
            self.diagnostics
                .extend(rule.check_fragment_children(fragment, self.ctx.source_text()));
        }
    }
}

impl<'a> Visit<'a> for LintRunner<'a> {
    fn visit_jsx_element(&mut self, element: &JSXElement<'a>) {
        self.check_jsx_element(element);
        walk::walk_jsx_element(self, element);
    }

    fn visit_jsx_opening_element(&mut self, opening: &JSXOpeningElement<'a>) {
        self.check_jsx_opening_element(opening);
        walk::walk_jsx_opening_element(self, opening);
    }

    fn visit_jsx_fragment(&mut self, fragment: &JSXFragment<'a>) {
        self.check_jsx_fragment(fragment);
        walk::walk_jsx_fragment(self, fragment);
    }
}

/// Result of running the linter
#[derive(Debug)]
pub struct LintResult {
    pub diagnostics: Vec<Diagnostic>,
    pub used_vars: Vec<String>,
}

impl LintResult {
    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|d| matches!(d.severity, crate::DiagnosticSeverity::Error))
    }

    pub fn has_warnings(&self) -> bool {
        !self.diagnostics.is_empty()
    }

    pub fn error_count(&self) -> usize {
        self.diagnostics
            .iter()
            .filter(|d| matches!(d.severity, crate::DiagnosticSeverity::Error))
            .count()
    }

    pub fn warning_count(&self) -> usize {
        self.diagnostics
            .iter()
            .filter(|d| matches!(d.severity, crate::DiagnosticSeverity::Warning))
            .count()
    }
}

/// Convenience function to lint a program with default configuration
pub fn lint<'a>(source_text: &'a str, program: &Program<'a>) -> LintResult {
    let ctx = VisitorLintContext::new(source_text, SourceType::jsx());
    let config = RulesConfig::default();
    LintRunner::new(ctx, config).run(program)
}

/// Convenience function to lint a program with custom configuration
pub fn lint_with_config<'a>(
    source_text: &'a str,
    source_type: SourceType,
    program: &Program<'a>,
    config: RulesConfig,
) -> LintResult {
    let ctx = VisitorLintContext::new(source_text, source_type);
    LintRunner::new(ctx, config).run(program)
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxc_allocator::Allocator;
    use oxc_parser::Parser;

    fn parse_and_lint(source: &str) -> LintResult {
        let allocator = Allocator::default();
        let source_type = SourceType::jsx();
        let ret = Parser::new(&allocator, source, source_type).parse();
        lint(source, &ret.program)
    }

    fn parse_and_lint_with_config(source: &str, config: RulesConfig) -> LintResult {
        let allocator = Allocator::default();
        let source_type = SourceType::jsx();
        let ret = Parser::new(&allocator, source, source_type).parse();
        lint_with_config(source, source_type, &ret.program, config)
    }

    #[test]
    fn test_lint_clean_code() {
        let result = parse_and_lint(r#"<div class="foo" />"#);
        assert!(result.diagnostics.is_empty());
    }

    #[test]
    fn test_lint_duplicate_props() {
        let result = parse_and_lint(r#"<div class="foo" class="bar" />"#);
        assert_eq!(result.diagnostics.len(), 1);
        assert!(result.diagnostics[0].message.contains("class"));
    }

    #[test]
    fn test_lint_react_props() {
        let result = parse_and_lint(r#"<div className="foo" />"#);
        assert_eq!(result.diagnostics.len(), 1);
        assert!(result.diagnostics[0].message.contains("class"));
    }

    #[test]
    fn test_lint_self_closing() {
        let result = parse_and_lint(r#"<div></div>"#);
        assert_eq!(result.diagnostics.len(), 1);
        assert!(result.diagnostics[0].message.contains("self-closing"));
    }

    #[test]
    fn test_lint_style_prop() {
        let result = parse_and_lint(r#"<div style={{ fontSize: "12px" }} />"#);
        assert_eq!(result.diagnostics.len(), 1);
        assert!(result.diagnostics[0].message.contains("font-size"));
    }

    #[test]
    fn test_lint_used_vars() {
        let result = parse_and_lint(r#"<MyComponent use:tooltip />"#);
        assert_eq!(result.used_vars, vec!["MyComponent", "tooltip"]);
    }

    #[test]
    fn test_lint_with_disabled_rules() {
        let config = RulesConfig::none().with_no_react_specific_props(true);
        let result = parse_and_lint_with_config(r#"<div className="foo" class="bar" />"#, config);
        // Only className warning, not duplicate class (that rule is disabled)
        assert_eq!(result.diagnostics.len(), 1);
        assert!(result.diagnostics[0].message.contains("className"));
    }

    #[test]
    fn test_lint_nested_elements() {
        let result = parse_and_lint(
            r#"<div className="outer"><span className="inner"></span></div>"#,
        );
        // Should find className issues in both elements, plus self-closing for span
        assert!(result.diagnostics.len() >= 2);
    }

    #[test]
    fn test_lint_fragment() {
        let result = parse_and_lint(r#"<>{items.map(x => <div />)}</>"#);
        assert_eq!(result.diagnostics.len(), 1);
        assert!(result.diagnostics[0].message.contains("For"));
    }

    #[test]
    fn test_result_counts() {
        let result = parse_and_lint(r#"<div className="a" className="b" />"#);
        assert!(result.has_warnings());
        assert!(!result.has_errors());
        assert_eq!(result.error_count(), 0);
        assert!(result.warning_count() > 0);
    }
}
