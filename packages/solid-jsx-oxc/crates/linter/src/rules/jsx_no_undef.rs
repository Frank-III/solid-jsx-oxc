//! solid/jsx-no-undef
//!
//! Disallow references to undefined variables in JSX.
//! Handles custom directives with use:X namespace.

use oxc_ast::ast::{
    JSXAttributeItem, JSXAttributeName, JSXElementName, JSXMemberExpressionObject,
    JSXOpeningElement, Program, Statement,
};
use oxc_semantic::{ScopeId, Scoping};
use oxc_span::Span;

use crate::diagnostic::{Diagnostic, Fix};
use crate::utils::is_dom_element;
use crate::{RuleCategory, RuleMeta};

/// Solid control flow components that can be auto-imported from "solid-js"
const AUTO_COMPONENTS: &[&str] = &["Show", "For", "Index", "Switch", "Match"];
const SOURCE_MODULE: &str = "solid-js";

/// Options for the jsx-no-undef rule
#[derive(Debug, Clone)]
pub struct JsxNoUndefOptions {
    /// When true, consider global scope when checking for defined components
    pub allow_globals: bool,
    /// Automatically suggest importing Solid components (Show, For, etc.)
    pub auto_import: bool,
    /// Don't report if TypeScript will catch undefined references
    pub typescript_enabled: bool,
}

impl Default for JsxNoUndefOptions {
    fn default() -> Self {
        Self {
            allow_globals: false,
            auto_import: true,
            typescript_enabled: false,
        }
    }
}

/// jsx-no-undef rule
#[derive(Debug, Clone, Default)]
pub struct JsxNoUndef {
    options: JsxNoUndefOptions,
}

impl RuleMeta for JsxNoUndef {
    const NAME: &'static str = "jsx-no-undef";
    const CATEGORY: RuleCategory = RuleCategory::Correctness;
}

/// Information about an undefined identifier
#[derive(Debug)]
struct UndefinedIdent {
    name: String,
    span: Span,
    is_component: bool,
    is_custom_directive: bool,
}

impl JsxNoUndef {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_options(options: JsxNoUndefOptions) -> Self {
        Self { options }
    }

    /// Check a JSX opening element for undefined references
    pub fn check<'a>(
        &self,
        opening: &JSXOpeningElement<'a>,
        scoping: &Scoping,
        scope_id: ScopeId,
    ) -> Vec<UndefinedIdent> {
        let mut undefined = Vec::new();

        // Check the element name
        match &opening.name {
            JSXElementName::Identifier(ident) => {
                // Only check components (capitalized names), not DOM elements
                if !is_dom_element(&ident.name) && ident.name != "this" {
                    if !self.is_defined(scoping, scope_id, &ident.name) {
                        undefined.push(UndefinedIdent {
                            name: ident.name.to_string(),
                            span: ident.span,
                            is_component: true,
                            is_custom_directive: false,
                        });
                    }
                }
            }
            JSXElementName::IdentifierReference(ident) => {
                if !is_dom_element(&ident.name) && ident.name != "this" {
                    if !self.is_defined(scoping, scope_id, &ident.name) {
                        undefined.push(UndefinedIdent {
                            name: ident.name.to_string(),
                            span: ident.span,
                            is_component: true,
                            is_custom_directive: false,
                        });
                    }
                }
            }
            JSXElementName::MemberExpression(member) => {
                // For <Foo.Bar>, check if Foo is defined
                if let Some((name, span)) = get_member_root(member) {
                    if name != "this" && !self.is_defined(scoping, scope_id, &name) {
                        undefined.push(UndefinedIdent {
                            name,
                            span,
                            is_component: false,
                            is_custom_directive: false,
                        });
                    }
                }
            }
            JSXElementName::NamespacedName(_) | JSXElementName::ThisExpression(_) => {
                // Namespaced names and this expressions don't need checking
            }
        }

        // Check use:X custom directives
        for attr in &opening.attributes {
            if let JSXAttributeItem::Attribute(jsx_attr) = attr {
                if let JSXAttributeName::NamespacedName(ns_name) = &jsx_attr.name {
                    if ns_name.namespace.name == "use" {
                        let directive_name = &ns_name.name.name;
                        if !self.is_defined(scoping, scope_id, directive_name) {
                            undefined.push(UndefinedIdent {
                                name: directive_name.to_string(),
                                span: ns_name.name.span,
                                is_component: false,
                                is_custom_directive: true,
                            });
                        }
                    }
                }
            }
        }

        undefined
    }

    /// Check if an identifier is defined in scope
    fn is_defined(&self, scoping: &Scoping, scope_id: ScopeId, name: &str) -> bool {
        // Check local/module scopes
        if scoping.find_binding(scope_id, name).is_some() {
            return true;
        }

        // Check global scope if allowed
        if self.options.allow_globals {
            if scoping.get_root_binding(name).is_some() {
                return true;
            }
        }

        false
    }

    /// Generate diagnostics from undefined identifiers
    pub fn generate_diagnostics(&self, undefined: Vec<UndefinedIdent>) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let mut missing_auto_imports: Vec<String> = Vec::new();

        for ident in undefined {
            if ident.is_custom_directive {
                diagnostics.push(Diagnostic::error(
                    Self::NAME,
                    ident.span,
                    format!("Custom directive '{}' is not defined.", ident.name),
                ));
            } else if ident.is_component
                && self.options.auto_import
                && AUTO_COMPONENTS.contains(&ident.name.as_str())
            {
                // Track for auto-import suggestion
                if !missing_auto_imports.contains(&ident.name) {
                    missing_auto_imports.push(ident.name);
                }
            } else if !self.options.typescript_enabled {
                diagnostics.push(Diagnostic::error(
                    Self::NAME,
                    ident.span,
                    format!("'{}' is not defined.", ident.name),
                ));
            }
        }

        // Generate auto-import diagnostic if there are missing Solid components
        if !missing_auto_imports.is_empty() {
            missing_auto_imports.sort();
            let imports_str = format_list(&missing_auto_imports);
            let import_statement =
                format!("import {{ {} }} from \"{}\";", missing_auto_imports.join(", "), SOURCE_MODULE);

            let mut diagnostic = Diagnostic::error(
                Self::NAME,
                Span::new(0, 0),
                format!("{} should be imported from '{}'.", imports_str, SOURCE_MODULE),
            )
            .with_help(format!("Add: {}", import_statement));

            // Add fix to insert import at top of file
            diagnostic = diagnostic.with_fix(
                Fix::new(Span::new(0, 0), format!("{}\n", import_statement))
                    .with_message(format!("Import {} from {}", imports_str, SOURCE_MODULE)),
            );

            diagnostics.push(diagnostic);
        }

        diagnostics
    }

    /// High-level check that processes an opening element and returns diagnostics
    pub fn check_and_report<'a>(
        &self,
        opening: &JSXOpeningElement<'a>,
        scoping: &Scoping,
        scope_id: ScopeId,
    ) -> Vec<Diagnostic> {
        let undefined = self.check(opening, scoping, scope_id);
        self.generate_diagnostics(undefined)
    }

    /// Check if an existing solid-js import exists and return its span for appending
    pub fn find_solid_import<'a>(program: &Program<'a>) -> Option<Span> {
        for stmt in &program.body {
            if let Statement::ImportDeclaration(import) = stmt {
                if import.source.value == SOURCE_MODULE {
                    return Some(import.span);
                }
            }
        }
        None
    }
}

/// Get the root identifier from a JSX member expression
fn get_member_root(member: &oxc_ast::ast::JSXMemberExpression) -> Option<(String, Span)> {
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

/// Format a list of items for display (e.g., "Show, For, and Index")
fn format_list(items: &[String]) -> String {
    match items.len() {
        0 => String::new(),
        1 => items[0].clone(),
        2 => format!("{} and {}", items[0], items[1]),
        _ => {
            let (last, rest) = items.split_last().unwrap();
            format!("{}, and {}", rest.join(", "), last)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rule_name() {
        assert_eq!(JsxNoUndef::NAME, "jsx-no-undef");
    }

    #[test]
    fn test_format_list() {
        assert_eq!(format_list(&[]), "");
        assert_eq!(format_list(&["Show".to_string()]), "Show");
        assert_eq!(format_list(&["Show".to_string(), "For".to_string()]), "Show and For");
        assert_eq!(
            format_list(&["Show".to_string(), "For".to_string(), "Index".to_string()]),
            "Show, For, and Index"
        );
    }

    #[test]
    fn test_default_options() {
        let options = JsxNoUndefOptions::default();
        assert!(!options.allow_globals);
        assert!(options.auto_import);
        assert!(!options.typescript_enabled);
    }
}
