//! SSR component transform
//!
//! Components in SSR are rendered using `createComponent`. Like DOM mode, we build a props object
//! where dynamic values are exposed as getters so they're evaluated inside reactive contexts.

use oxc_ast::ast::{
    Argument, ArrayExpressionElement, Expression, FormalParameterKind, FunctionType,
    JSXAttributeItem, JSXAttributeName, JSXAttributeValue, JSXChild, JSXElement, JSXElementName,
    JSXMemberExpression, JSXMemberExpressionObject, ObjectPropertyKind, PropertyKey, PropertyKind,
    Statement,
};
use oxc_ast::AstBuilder;
use oxc_ast::NONE;
use oxc_span::SPAN;

use common::{is_dynamic, TransformOptions};

use crate::ir::{SSRChildTransformer, SSRContext, SSRResult};

fn jsx_member_expression_to_expression<'a>(
    ast: AstBuilder<'a>,
    member: &JSXMemberExpression<'a>,
) -> Expression<'a> {
    let object = match &member.object {
        JSXMemberExpressionObject::IdentifierReference(id) => {
            ast.expression_identifier(id.span, id.name)
        }
        JSXMemberExpressionObject::MemberExpression(inner) => {
            jsx_member_expression_to_expression(ast, inner)
        }
        JSXMemberExpressionObject::ThisExpression(expr) => ast.expression_this(expr.span),
    };

    let property = ast.identifier_name(member.property.span, member.property.name);
    Expression::StaticMemberExpression(ast.alloc_static_member_expression(
        member.span,
        object,
        property,
        false,
    ))
}

fn jsx_element_name_to_expression<'a>(
    ast: AstBuilder<'a>,
    name: &JSXElementName<'a>,
) -> Expression<'a> {
    match name {
        JSXElementName::Identifier(id) => ast.expression_identifier(id.span, id.name),
        JSXElementName::IdentifierReference(id) => ast.expression_identifier(id.span, id.name),
        JSXElementName::MemberExpression(member) => {
            jsx_member_expression_to_expression(ast, member)
        }
        JSXElementName::ThisExpression(expr) => ast.expression_this(expr.span),
        JSXElementName::NamespacedName(ns) => {
            // Namespaced tag names are not valid component references in JS.
            // This should never happen for components.
            let _ = ns;
            ast.expression_identifier(SPAN, "undefined")
        }
    }
}

fn getter_return_expr<'a>(
    ast: AstBuilder<'a>,
    span: oxc_span::Span,
    expr: Expression<'a>,
) -> Expression<'a> {
    let _ = span;
    let params =
        ast.alloc_formal_parameters(SPAN, FormalParameterKind::FormalParameter, ast.vec(), NONE);
    let mut statements = ast.vec_with_capacity(1);
    statements.push(Statement::ReturnStatement(
        ast.alloc_return_statement(SPAN, Some(expr)),
    ));
    let body = ast.alloc_function_body(SPAN, ast.vec(), statements);
    ast.expression_function(
        SPAN,
        FunctionType::FunctionExpression,
        None,
        false,
        false,
        false,
        NONE,
        NONE,
        params,
        NONE,
        Some(body),
    )
}

fn is_valid_prop_identifier(key: &str) -> bool {
    let mut chars = key.chars();
    match chars.next() {
        Some(c) if c == '$' || c == '_' || c.is_ascii_alphabetic() => {}
        _ => return false,
    }
    chars.all(|c| c == '$' || c == '_' || c.is_ascii_alphanumeric())
}

fn make_prop_key<'a>(ast: AstBuilder<'a>, span: oxc_span::Span, raw_key: &str) -> PropertyKey<'a> {
    let _ = span;
    let key = ast.allocator.alloc_str(raw_key);
    if is_valid_prop_identifier(raw_key) {
        PropertyKey::StaticIdentifier(ast.alloc_identifier_name(SPAN, key))
    } else {
        PropertyKey::StringLiteral(ast.alloc_string_literal(SPAN, key, None))
    }
}

/// Get children as SSR expression with recursive transformation
fn get_children_ssr<'a, 'b>(
    element: &JSXElement<'a>,
    context: &SSRContext<'a>,
    transform_child: SSRChildTransformer<'a, 'b>,
) -> Expression<'a> {
    let ast = context.ast();
    let mut children: Vec<Expression<'a>> = Vec::new();

    for child in &element.children {
        match child {
            JSXChild::Text(text) => {
                let content = common::expression::trim_whitespace(&text.value);
                if !content.is_empty() {
                    let escaped = common::expression::escape_html(&content, false);
                    children.push(ast.expression_string_literal(
                        SPAN,
                        ast.allocator.alloc_str(&escaped),
                        None,
                    ));
                }
            }
            JSXChild::ExpressionContainer(container) => {
                if let Some(expr) = container.expression.as_expression() {
                    children.push(context.clone_expr(expr));
                }
            }
            JSXChild::Element(_) | JSXChild::Fragment(_) => {
                // Transform the child JSX element/fragment
                if let Some(result) = transform_child(child) {
                    children.push(result.to_ssr_expression(ast, false));
                }
            }
            JSXChild::Spread(spread) => {
                children.push(context.clone_expr(&spread.expression));
            }
        }
    }

    if children.len() == 1 {
        children
            .pop()
            .unwrap_or_else(|| ast.expression_identifier(SPAN, "undefined"))
    } else if children.is_empty() {
        ast.expression_identifier(SPAN, "undefined")
    } else {
        let mut elements = ast.vec_with_capacity(children.len());
        for expr in children {
            elements.push(ArrayExpressionElement::from(expr));
        }
        ast.expression_array(SPAN, elements)
    }
}

/// Transform a component for SSR
pub fn transform_component<'a, 'b>(
    element: &JSXElement<'a>,
    _tag_name: &str,
    context: &SSRContext<'a>,
    options: &TransformOptions<'a>,
    transform_child: SSRChildTransformer<'a, 'b>,
) -> SSRResult<'a> {
    let ast = context.ast();
    let mut result = SSRResult::new();
    result.span = element.span;

    context.register_helper("createComponent");
    context.register_helper("escape");

    // Build props
    let props = build_props(element, context, options, transform_child);

    // Generate createComponent call - will be escaped by parent
    let component = jsx_element_name_to_expression(ast, &element.opening_element.name);
    let callee = ast.expression_identifier(SPAN, "createComponent");
    let mut args = ast.vec_with_capacity(2);
    args.push(Argument::from(component));
    args.push(Argument::from(props));
    let call = ast.expression_call(
        SPAN,
        callee,
        None::<oxc_ast::ast::TSTypeParameterInstantiation<'a>>,
        args,
        false,
    );
    result.push_dynamic(
        call, false, false, // Components return escaped content
    );

    result
}

/// Build props object for a component
fn build_props<'a, 'b>(
    element: &JSXElement<'a>,
    context: &SSRContext<'a>,
    _options: &TransformOptions<'a>,
    transform_child: SSRChildTransformer<'a, 'b>,
) -> Expression<'a> {
    let ast = context.ast();
    let span = SPAN;

    let mut static_props: Vec<ObjectPropertyKind<'a>> = Vec::new();
    let mut dynamic_props: Vec<ObjectPropertyKind<'a>> = Vec::new();
    let mut spreads: Vec<Expression<'a>> = Vec::new();

    for attr in &element.opening_element.attributes {
        match attr {
            JSXAttributeItem::Attribute(attr) => {
                let raw_key: String = match &attr.name {
                    JSXAttributeName::Identifier(id) => id.name.as_str().to_string(),
                    JSXAttributeName::NamespacedName(ns) => {
                        format!("{}:{}", ns.namespace.name, ns.name.name)
                    }
                };
                let key = make_prop_key(ast, span, &raw_key);

                // Skip event handlers and refs in SSR
                if raw_key.starts_with("on") || raw_key == "ref" || raw_key.starts_with("use:") {
                    continue;
                }

                // Ignore explicit `children` prop when actual JSX children exist.
                // JSX children win over `children={...}`.
                if raw_key == "children" && !element.children.is_empty() {
                    continue;
                }

                match &attr.value {
                    Some(JSXAttributeValue::StringLiteral(lit)) => {
                        static_props.push(ast.object_property_kind_object_property(
                            span,
                            PropertyKind::Init,
                            key,
                            ast.expression_string_literal(
                                span,
                                ast.allocator.alloc_str(&lit.value),
                                None,
                            ),
                            false,
                            false,
                            false,
                        ));
                    }
                    Some(JSXAttributeValue::ExpressionContainer(container)) => {
                        if let Some(expr) = container.expression.as_expression() {
                            if is_dynamic(expr) {
                                dynamic_props.push(ast.object_property_kind_object_property(
                                    span,
                                    PropertyKind::Get,
                                    key,
                                    getter_return_expr(ast, span, context.clone_expr(expr)),
                                    false,
                                    false,
                                    false,
                                ));
                            } else {
                                static_props.push(ast.object_property_kind_object_property(
                                    span,
                                    PropertyKind::Init,
                                    key,
                                    context.clone_expr(expr),
                                    false,
                                    false,
                                    false,
                                ));
                            }
                        }
                    }
                    None => {
                        static_props.push(ast.object_property_kind_object_property(
                            span,
                            PropertyKind::Init,
                            key,
                            ast.expression_boolean_literal(span, true),
                            false,
                            false,
                            false,
                        ));
                    }
                    _ => {}
                }
            }
            JSXAttributeItem::SpreadAttribute(spread) => {
                spreads.push(context.clone_expr(&spread.argument));
            }
        }
    }

    // Handle children
    if !element.children.is_empty() {
        let children = get_children_ssr(element, context, transform_child);
        let key = make_prop_key(ast, span, "children");
        if is_dynamic(&children) {
            let getter = getter_return_expr(ast, span, children);
            dynamic_props.push(ast.object_property_kind_object_property(
                span,
                PropertyKind::Get,
                key,
                getter,
                false,
                false,
                false,
            ));
        } else {
            static_props.push(ast.object_property_kind_object_property(
                span,
                PropertyKind::Init,
                key,
                children,
                false,
                false,
                false,
            ));
        }
    }

    let has_inline_props = !static_props.is_empty() || !dynamic_props.is_empty();
    let mut props = ast.vec_with_capacity(static_props.len() + dynamic_props.len());
    for prop in static_props {
        props.push(prop);
    }
    for prop in dynamic_props {
        props.push(prop);
    }

    // Combine props
    if !spreads.is_empty() {
        context.register_helper("mergeProps");
        let callee = ast.expression_identifier(span, "mergeProps");
        let mut args = ast.vec_with_capacity(spreads.len() + if has_inline_props { 1 } else { 0 });
        for spread in spreads {
            args.push(Argument::from(spread));
        }
        if has_inline_props {
            args.push(Argument::from(ast.expression_object(span, props)));
        }
        ast.expression_call(
            span,
            callee,
            None::<oxc_ast::ast::TSTypeParameterInstantiation<'a>>,
            args,
            false,
        )
    } else {
        ast.expression_object(span, props)
    }
}
