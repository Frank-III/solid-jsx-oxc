//! solid/jsx-uses-vars
//!
//! Marks variables used in JSX elements as "used" to prevent false positives
//! from no-unused-vars rules.
//!
//! This rule collects variable names used in:
//! - JSX element names (`<Foo>` marks `Foo` as used)
//! - JSX member expressions (`<Foo.Bar.Baz>` marks `Foo` as used)
//! - Custom directives (`use:tooltip` marks `tooltip` as used)

use oxc_ast::ast::{
    JSXAttributeItem, JSXAttributeName, JSXElementName, JSXMemberExpressionObject,
    JSXOpeningElement,
};

use crate::{RuleCategory, RuleMeta};

/// jsx-uses-vars rule
#[derive(Debug, Clone, Default)]
pub struct JsxUsesVars;

impl RuleMeta for JsxUsesVars {
    const NAME: &'static str = "jsx-uses-vars";
    const CATEGORY: RuleCategory = RuleCategory::Correctness;
}

impl JsxUsesVars {
    pub fn new() -> Self {
        Self
    }

    /// Collect variable names used in a JSX opening element.
    /// Returns a Vec of variable names that should be marked as "used".
    pub fn collect_used_vars<'a>(&self, opening: &JSXOpeningElement<'a>) -> Vec<String> {
        let mut used = Vec::new();

        // Check element name
        match &opening.name {
            JSXElementName::Identifier(ident) => {
                // Only mark as used if it's a component (capitalized)
                if ident.name.chars().next().is_some_and(|c| c.is_uppercase()) {
                    used.push(ident.name.to_string());
                }
            }
            JSXElementName::IdentifierReference(ident) => {
                if ident.name.chars().next().is_some_and(|c| c.is_uppercase()) {
                    used.push(ident.name.to_string());
                }
            }
            JSXElementName::MemberExpression(member) => {
                // Traverse to root identifier
                let mut current = &member.object;
                loop {
                    match current {
                        JSXMemberExpressionObject::IdentifierReference(ident) => {
                            used.push(ident.name.to_string());
                            break;
                        }
                        JSXMemberExpressionObject::MemberExpression(inner) => {
                            current = &inner.object;
                        }
                        JSXMemberExpressionObject::ThisExpression(_) => {
                            // `this` is not a variable, skip
                            break;
                        }
                    }
                }
            }
            JSXElementName::NamespacedName(_) => {
                // Skip <Foo:Bar> - namespaced elements don't reference variables
            }
            JSXElementName::ThisExpression(_) => {
                // `this` is not a variable
            }
        }

        // Check for use:X directives
        for attr in &opening.attributes {
            if let JSXAttributeItem::Attribute(jsx_attr) = attr {
                if let JSXAttributeName::NamespacedName(ns) = &jsx_attr.name {
                    if ns.namespace.name == "use" {
                        used.push(ns.name.name.to_string());
                    }
                }
            }
        }

        used
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxc_allocator::Allocator;
    use oxc_ast::ast::{Expression, JSXElement, Statement};
    use oxc_parser::Parser;
    use oxc_span::SourceType;

    fn find_jsx_element<'a>(program: &'a oxc_ast::ast::Program<'a>) -> Option<&'a JSXElement<'a>> {
        for stmt in &program.body {
            if let Statement::VariableDeclaration(decl) = stmt {
                for declarator in &decl.declarations {
                    if let Some(init) = &declarator.init {
                        if let Expression::JSXElement(elem) = init {
                            return Some(elem);
                        }
                    }
                }
            }
            if let Statement::ExpressionStatement(expr_stmt) = stmt {
                if let Expression::JSXElement(elem) = &expr_stmt.expression {
                    return Some(elem);
                }
            }
        }
        None
    }

    fn parse_and_collect_used_vars(code: &str) -> Vec<String> {
        let allocator = Allocator::default();
        let source_type = SourceType::jsx();
        let ret = Parser::new(&allocator, code, source_type).parse();
        let element = find_jsx_element(&ret.program).expect("should find JSX element");
        let rule = JsxUsesVars::new();
        rule.collect_used_vars(&element.opening_element)
    }

    #[test]
    fn test_simple_component() {
        let used = parse_and_collect_used_vars("const x = <Foo />;");
        assert_eq!(used, vec!["Foo"]);
    }

    #[test]
    fn test_member_expression() {
        let used = parse_and_collect_used_vars("const x = <Foo.Bar.Baz />;");
        assert_eq!(used, vec!["Foo"]);
    }

    #[test]
    fn test_use_directive() {
        let used = parse_and_collect_used_vars("const x = <div use:tooltip />;");
        assert_eq!(used, vec!["tooltip"]);
    }

    #[test]
    fn test_namespaced_element_skipped() {
        let used = parse_and_collect_used_vars("const x = <foo:bar />;");
        assert!(used.is_empty());
    }

    #[test]
    fn test_lowercase_element_skipped() {
        let used = parse_and_collect_used_vars("const x = <div />;");
        assert!(used.is_empty());
    }

    #[test]
    fn test_multiple_use_directives() {
        let used = parse_and_collect_used_vars("const x = <div use:tooltip use:clickOutside />;");
        assert_eq!(used, vec!["tooltip", "clickOutside"]);
    }

    #[test]
    fn test_component_with_use_directive() {
        let used = parse_and_collect_used_vars("const x = <MyComponent use:tooltip />;");
        assert_eq!(used, vec!["MyComponent", "tooltip"]);
    }
}
