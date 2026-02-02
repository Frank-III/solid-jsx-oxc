//! Integration tests for solid-linter rules

use oxc_allocator::Allocator;
use oxc_ast::ast::JSXElement;
use oxc_parser::Parser;
use oxc_span::SourceType;

use solid_linter::rules::{
    JsxNoDuplicateProps, NoInnerhtml, NoReactDeps, NoReactSpecificProps, NoUnknownNamespaces,
    PreferClasslist, SelfClosingComp, StyleProp,
};

fn parse_jsx_element<'a>(allocator: &'a Allocator, source: &'a str) -> Option<oxc_ast::ast::Program<'a>> {
    let source_type = SourceType::jsx();
    let ret = Parser::new(allocator, source, source_type).parse();
    if ret.errors.is_empty() {
        Some(ret.program)
    } else {
        None
    }
}

/// Helper to find the first JSX element in a program
fn find_jsx_element<'a>(program: &'a oxc_ast::ast::Program<'a>) -> Option<&'a JSXElement<'a>> {
    // Simple traversal to find JSX element - in real usage, use a visitor
    for stmt in &program.body {
        if let oxc_ast::ast::Statement::ExpressionStatement(expr_stmt) = stmt {
            if let oxc_ast::ast::Expression::JSXElement(elem) = &expr_stmt.expression {
                return Some(elem);
            }
        }
    }
    None
}

#[test]
fn test_jsx_no_duplicate_props_pass() {
    let allocator = Allocator::default();
    let source = r#"<div class="foo" id="bar" />"#;
    
    let program = parse_jsx_element(&allocator, source).expect("should parse");
    let element = find_jsx_element(&program).expect("should find element");
    
    let rule = JsxNoDuplicateProps::new();
    let diagnostics = rule.check(&element.opening_element, &element.children);
    
    assert!(diagnostics.is_empty(), "should have no diagnostics");
}

#[test]
fn test_jsx_no_duplicate_props_fail_duplicate() {
    let allocator = Allocator::default();
    let source = r#"<div class="foo" class="bar" />"#;
    
    let program = parse_jsx_element(&allocator, source).expect("should parse");
    let element = find_jsx_element(&program).expect("should find element");
    
    let rule = JsxNoDuplicateProps::new();
    let diagnostics = rule.check(&element.opening_element, &element.children);
    
    assert_eq!(diagnostics.len(), 1, "should have one diagnostic");
    assert!(diagnostics[0].message.contains("class"));
}

#[test]
fn test_jsx_no_duplicate_props_fail_children_conflict() {
    let allocator = Allocator::default();
    let source = r#"<div innerHTML="test">Hello</div>"#;
    
    let program = parse_jsx_element(&allocator, source).expect("should parse");
    let element = find_jsx_element(&program).expect("should find element");
    
    let rule = JsxNoDuplicateProps::new();
    let diagnostics = rule.check(&element.opening_element, &element.children);
    
    assert_eq!(diagnostics.len(), 1, "should have one diagnostic");
    assert!(diagnostics[0].message.contains("innerHTML"));
}

#[test]
fn test_no_react_specific_props_class_name() {
    let allocator = Allocator::default();
    let source = r#"<div className="foo" />"#;
    
    let program = parse_jsx_element(&allocator, source).expect("should parse");
    let element = find_jsx_element(&program).expect("should find element");
    
    let rule = NoReactSpecificProps::new();
    let diagnostics = rule.check(&element.opening_element);
    
    assert_eq!(diagnostics.len(), 1, "should have one diagnostic");
    assert!(diagnostics[0].message.contains("class"));
    assert!(diagnostics[0].fixes.len() > 0, "should have a fix");
}

#[test]
fn test_no_react_specific_props_key() {
    let allocator = Allocator::default();
    let source = r#"<li key={item.id}>text</li>"#;
    
    let program = parse_jsx_element(&allocator, source).expect("should parse");
    let element = find_jsx_element(&program).expect("should find element");
    
    let rule = NoReactSpecificProps::new();
    let diagnostics = rule.check(&element.opening_element);
    
    assert_eq!(diagnostics.len(), 1, "should have one diagnostic");
    assert!(diagnostics[0].message.contains("key"));
}

#[test]
fn test_self_closing_comp_empty_div() {
    let allocator = Allocator::default();
    let source = r#"<div></div>"#;
    
    let program = parse_jsx_element(&allocator, source).expect("should parse");
    let element = find_jsx_element(&program).expect("should find element");
    
    let rule = SelfClosingComp::new();
    let closing_span = element.closing_element.as_ref().map(|c| c.span);
    let diagnostics = rule.check(&element.opening_element, &element.children, closing_span);
    
    assert_eq!(diagnostics.len(), 1, "should have one diagnostic");
    assert!(diagnostics[0].message.contains("self-closing"));
}

#[test]
fn test_self_closing_comp_already_self_closing() {
    let allocator = Allocator::default();
    let source = r#"<div />"#;
    
    let program = parse_jsx_element(&allocator, source).expect("should parse");
    let element = find_jsx_element(&program).expect("should find element");
    
    let rule = SelfClosingComp::new();
    let closing_span = element.closing_element.as_ref().map(|c| c.span);
    let diagnostics = rule.check(&element.opening_element, &element.children, closing_span);
    
    assert!(diagnostics.is_empty(), "should have no diagnostics");
}

#[test]
fn test_self_closing_comp_with_children() {
    let allocator = Allocator::default();
    let source = r#"<div>Hello</div>"#;
    
    let program = parse_jsx_element(&allocator, source).expect("should parse");
    let element = find_jsx_element(&program).expect("should find element");
    
    let rule = SelfClosingComp::new();
    let closing_span = element.closing_element.as_ref().map(|c| c.span);
    let diagnostics = rule.check(&element.opening_element, &element.children, closing_span);
    
    assert!(diagnostics.is_empty(), "should have no diagnostics - has children");
}

// ============ no-innerhtml tests ============

#[test]
fn test_no_innerhtml_dangerous() {
    let allocator = Allocator::default();
    let source = r#"<div innerHTML={userInput} />"#;
    
    let program = parse_jsx_element(&allocator, source).expect("should parse");
    let element = find_jsx_element(&program).expect("should find element");
    
    let rule = NoInnerhtml::new();
    let diagnostics = rule.check(element);
    
    assert_eq!(diagnostics.len(), 1, "should warn about dynamic innerHTML");
    assert!(diagnostics[0].message.contains("dangerous"));
}

#[test]
fn test_no_innerhtml_static_html_allowed() {
    let allocator = Allocator::default();
    let source = r#"<div innerHTML="<b>bold</b>" />"#;
    
    let program = parse_jsx_element(&allocator, source).expect("should parse");
    let element = find_jsx_element(&program).expect("should find element");
    
    let rule = NoInnerhtml::new(); // allow_static defaults to true
    let diagnostics = rule.check(element);
    
    assert!(diagnostics.is_empty(), "static HTML should be allowed");
}

#[test]
fn test_no_innerhtml_dangerously_set() {
    let allocator = Allocator::default();
    let source = r#"<div dangerouslySetInnerHTML={{ __html: content }} />"#;
    
    let program = parse_jsx_element(&allocator, source).expect("should parse");
    let element = find_jsx_element(&program).expect("should find element");
    
    let rule = NoInnerhtml::new();
    let diagnostics = rule.check(element);
    
    assert_eq!(diagnostics.len(), 1, "should warn about dangerouslySetInnerHTML");
    assert!(diagnostics[0].message.contains("dangerouslySetInnerHTML"));
}

// ============ no-unknown-namespaces tests ============

#[test]
fn test_no_unknown_namespaces_valid() {
    let allocator = Allocator::default();
    let source = r#"<div on:click={handler} />"#;
    
    let program = parse_jsx_element(&allocator, source).expect("should parse");
    let element = find_jsx_element(&program).expect("should find element");
    
    let rule = NoUnknownNamespaces::new();
    let diagnostics = rule.check(&element.opening_element);
    
    assert!(diagnostics.is_empty(), "on: namespace should be valid");
}

#[test]
fn test_no_unknown_namespaces_invalid() {
    let allocator = Allocator::default();
    let source = r#"<div foo:bar={value} />"#;
    
    let program = parse_jsx_element(&allocator, source).expect("should parse");
    let element = find_jsx_element(&program).expect("should find element");
    
    let rule = NoUnknownNamespaces::new();
    let diagnostics = rule.check(&element.opening_element);
    
    assert_eq!(diagnostics.len(), 1, "foo: should be unknown");
    assert!(diagnostics[0].message.contains("foo:"));
}

#[test]
fn test_no_unknown_namespaces_on_component() {
    let allocator = Allocator::default();
    let source = r#"<MyComponent on:click={handler} />"#;
    
    let program = parse_jsx_element(&allocator, source).expect("should parse");
    let element = find_jsx_element(&program).expect("should find element");
    
    let rule = NoUnknownNamespaces::new();
    let diagnostics = rule.check(&element.opening_element);
    
    assert_eq!(diagnostics.len(), 1, "namespaces on components have no effect");
    assert!(diagnostics[0].message.contains("no effect"));
}

// ============ style-prop tests ============

#[test]
fn test_style_prop_kebab_case() {
    let allocator = Allocator::default();
    let source = r#"<div style={{ fontSize: "12px" }} />"#;
    
    let program = parse_jsx_element(&allocator, source).expect("should parse");
    let element = find_jsx_element(&program).expect("should find element");
    
    let rule = StyleProp::new();
    let diagnostics = rule.check(&element.opening_element);
    
    assert_eq!(diagnostics.len(), 1, "should warn about camelCase");
    assert!(diagnostics[0].message.contains("font-size"));
}

#[test]
fn test_style_prop_string() {
    let allocator = Allocator::default();
    let source = r#"<div style="color: red" />"#;
    
    let program = parse_jsx_element(&allocator, source).expect("should parse");
    let element = find_jsx_element(&program).expect("should find element");
    
    let rule = StyleProp::new(); // allow_string defaults to false
    let diagnostics = rule.check(&element.opening_element);
    
    assert_eq!(diagnostics.len(), 1, "should warn about string style");
    assert!(diagnostics[0].message.contains("object"));
}

#[test]
fn test_style_prop_valid() {
    let allocator = Allocator::default();
    let source = r#"<div style={{ "font-size": "12px", color: "red" }} />"#;
    
    let program = parse_jsx_element(&allocator, source).expect("should parse");
    let element = find_jsx_element(&program).expect("should find element");
    
    let rule = StyleProp::new();
    let diagnostics = rule.check(&element.opening_element);
    
    assert!(diagnostics.is_empty(), "valid style should pass");
}

// ============ prefer-classlist tests ============

#[test]
fn test_prefer_classlist_clsx() {
    let allocator = Allocator::default();
    let source = r#"<div class={clsx({ active: isActive })} />"#;
    
    let program = parse_jsx_element(&allocator, source).expect("should parse");
    let element = find_jsx_element(&program).expect("should find element");
    
    let rule = PreferClasslist::new();
    let diagnostics = rule.check(&element.opening_element);
    
    assert_eq!(diagnostics.len(), 1, "should warn about clsx");
    assert!(diagnostics[0].message.contains("clsx"));
    assert!(diagnostics[0].message.contains("classlist"));
}

#[test]
fn test_prefer_classlist_cn() {
    let allocator = Allocator::default();
    let source = r#"<div className={cn({ foo: true, bar: false })} />"#;
    
    let program = parse_jsx_element(&allocator, source).expect("should parse");
    let element = find_jsx_element(&program).expect("should find element");
    
    let rule = PreferClasslist::new();
    let diagnostics = rule.check(&element.opening_element);
    
    assert_eq!(diagnostics.len(), 1, "should warn about cn");
    assert!(diagnostics[0].message.contains("cn"));
}

#[test]
fn test_prefer_classlist_already_has_classlist() {
    let allocator = Allocator::default();
    let source = r#"<div class="base" classList={{ active: isActive }} />"#;
    
    let program = parse_jsx_element(&allocator, source).expect("should parse");
    let element = find_jsx_element(&program).expect("should find element");
    
    let rule = PreferClasslist::new();
    let diagnostics = rule.check(&element.opening_element);
    
    assert!(diagnostics.is_empty(), "should not warn when classList exists");
}

#[test]
fn test_prefer_classlist_plain_class() {
    let allocator = Allocator::default();
    let source = r#"<div class="foo bar" />"#;
    
    let program = parse_jsx_element(&allocator, source).expect("should parse");
    let element = find_jsx_element(&program).expect("should find element");
    
    let rule = PreferClasslist::new();
    let diagnostics = rule.check(&element.opening_element);
    
    assert!(diagnostics.is_empty(), "should not warn about plain class strings");
}

// ============ no-react-deps tests ============

fn find_call_expression<'a>(program: &'a oxc_ast::ast::Program<'a>) -> Option<&'a oxc_ast::ast::CallExpression<'a>> {
    for stmt in &program.body {
        if let oxc_ast::ast::Statement::ExpressionStatement(expr_stmt) = stmt {
            if let oxc_ast::ast::Expression::CallExpression(call) = &expr_stmt.expression {
                return Some(call);
            }
        }
    }
    None
}

#[test]
fn test_no_react_deps_valid_single_arg() {
    let allocator = Allocator::default();
    let source = r#"createEffect(() => { console.log(signal()); });"#;
    
    let program = parse_jsx_element(&allocator, source).expect("should parse");
    let call = find_call_expression(&program).expect("should find call");
    
    let rule = NoReactDeps::new();
    let diagnostics = rule.check(call);
    
    assert!(diagnostics.is_empty(), "single argument should be valid");
}

#[test]
fn test_no_react_deps_valid_with_initial_value() {
    let allocator = Allocator::default();
    let source = r#"createEffect((prev) => { console.log(signal()); return prev + 1; }, 0);"#;
    
    let program = parse_jsx_element(&allocator, source).expect("should parse");
    let call = find_call_expression(&program).expect("should find call");
    
    let rule = NoReactDeps::new();
    let diagnostics = rule.check(call);
    
    assert!(diagnostics.is_empty(), "function with params and initial value should be valid");
}

#[test]
fn test_no_react_deps_valid_memo_single_arg() {
    let allocator = Allocator::default();
    let source = r#"createMemo(() => computeExpensiveValue(a(), b()));"#;
    
    let program = parse_jsx_element(&allocator, source).expect("should parse");
    let call = find_call_expression(&program).expect("should find call");
    
    let rule = NoReactDeps::new();
    let diagnostics = rule.check(call);
    
    assert!(diagnostics.is_empty(), "single argument memo should be valid");
}

#[test]
fn test_no_react_deps_invalid_effect_with_deps() {
    let allocator = Allocator::default();
    let source = r#"createEffect(() => { console.log(signal()); }, [signal()]);"#;
    
    let program = parse_jsx_element(&allocator, source).expect("should parse");
    let call = find_call_expression(&program).expect("should find call");
    
    let rule = NoReactDeps::new();
    let diagnostics = rule.check(call);
    
    assert_eq!(diagnostics.len(), 1, "should have one diagnostic");
    assert!(diagnostics[0].message.contains("createEffect"));
    assert!(diagnostics[0].message.contains("dependency array"));
    assert!(!diagnostics[0].fixes.is_empty(), "should have a fix");
}

#[test]
fn test_no_react_deps_invalid_memo_with_deps() {
    let allocator = Allocator::default();
    let source = r#"createMemo(() => computeExpensiveValue(a(), b()), [a(), b()]);"#;
    
    let program = parse_jsx_element(&allocator, source).expect("should parse");
    let call = find_call_expression(&program).expect("should find call");
    
    let rule = NoReactDeps::new();
    let diagnostics = rule.check(call);
    
    assert_eq!(diagnostics.len(), 1, "should have one diagnostic");
    assert!(diagnostics[0].message.contains("createMemo"));
    assert!(!diagnostics[0].fixes.is_empty(), "should have a fix");
}

#[test]
fn test_no_react_deps_valid_other_function() {
    let allocator = Allocator::default();
    let source = r#"someOtherFunction(() => {}, [deps]);"#;
    
    let program = parse_jsx_element(&allocator, source).expect("should parse");
    let call = find_call_expression(&program).expect("should find call");
    
    let rule = NoReactDeps::new();
    let diagnostics = rule.check(call);
    
    assert!(diagnostics.is_empty(), "should not warn about other functions");
}
