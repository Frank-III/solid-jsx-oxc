//! Component transform
//! Handles <MyComponent /> -> createComponent(MyComponent, {...})

use oxc_allocator::CloneIn;
use oxc_ast::ast::{
    Argument, ArrayExpressionElement, AssignmentTarget, Expression, FormalParameterKind,
    FunctionType, JSXAttributeItem, JSXAttributeName, JSXAttributeValue, JSXChild, JSXElement,
    JSXElementName, JSXMemberExpression, JSXMemberExpressionObject, ObjectPropertyKind,
    PropertyKey, PropertyKind, Statement, VariableDeclarationKind,
};
use oxc_ast::AstBuilder;
use oxc_ast::NONE;
use oxc_span::SPAN;
use oxc_syntax::operator::{AssignmentOperator, BinaryOperator, UnaryOperator};

use common::{is_dynamic, TransformOptions};

use crate::ir::{BlockContext, ChildTransformer, TransformResult};
use crate::output::build_dom_output_expr;

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

fn expression_to_assignment_target<'a>(expr: Expression<'a>) -> Option<AssignmentTarget<'a>> {
    match expr {
        Expression::Identifier(ident) => Some(AssignmentTarget::AssignmentTargetIdentifier(ident)),
        Expression::StaticMemberExpression(m) => Some(AssignmentTarget::StaticMemberExpression(m)),
        Expression::ComputedMemberExpression(m) => {
            Some(AssignmentTarget::ComputedMemberExpression(m))
        }
        Expression::PrivateFieldExpression(m) => Some(AssignmentTarget::PrivateFieldExpression(m)),
        Expression::TSAsExpression(e) => Some(AssignmentTarget::TSAsExpression(e)),
        Expression::TSSatisfiesExpression(e) => Some(AssignmentTarget::TSSatisfiesExpression(e)),
        Expression::TSNonNullExpression(e) => Some(AssignmentTarget::TSNonNullExpression(e)),
        Expression::TSTypeAssertion(e) => Some(AssignmentTarget::TSTypeAssertion(e)),
        _ => None,
    }
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

/// Get children as an expression with recursive transformation.
fn get_children_expr_transformed<'a, 'b>(
    element: &JSXElement<'a>,
    context: &BlockContext<'a>,
    transform_child: ChildTransformer<'a, 'b>,
) -> Option<Expression<'a>> {
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
                if let Some(result) = transform_child(child) {
                    children.push(build_dom_output_expr(&result, context));
                }
            }
            JSXChild::Spread(spread) => {
                children.push(context.clone_expr(&spread.expression));
            }
        }
    }

    if children.len() == 1 {
        Some(
            children
                .pop()
                .unwrap_or_else(|| ast.expression_identifier(SPAN, "undefined")),
        )
    } else if children.is_empty() {
        None
    } else {
        let mut elements = ast.vec_with_capacity(children.len());
        for expr in children {
            elements.push(ArrayExpressionElement::from(expr));
        }
        Some(ast.expression_array(SPAN, elements))
    }
}

/// Transform a component element
pub fn transform_component<'a, 'b>(
    element: &JSXElement<'a>,
    _tag_name: &str,
    context: &BlockContext<'a>,
    options: &TransformOptions<'a>,
    transform_child: ChildTransformer<'a, 'b>,
) -> TransformResult<'a> {
    let ast = context.ast();
    let mut result = TransformResult {
        span: element.span,
        ..Default::default()
    };

    context.register_helper("createComponent");

    // Build props object
    let props = build_props(element, context, options, transform_child);

    // Generate createComponent call
    let callee = ast.expression_identifier(SPAN, "createComponent");
    let mut args = ast.vec_with_capacity(2);
    args.push(Argument::from(jsx_element_name_to_expression(
        ast,
        &element.opening_element.name,
    )));
    args.push(Argument::from(props));
    result.exprs.push(ast.expression_call(
        SPAN,
        callee,
        None::<oxc_ast::ast::TSTypeParameterInstantiation<'a>>,
        args,
        false,
    ));

    result
}

/// Build props object for a component.
fn build_props<'a, 'b>(
    element: &JSXElement<'a>,
    context: &BlockContext<'a>,
    _options: &TransformOptions<'a>,
    transform_child: ChildTransformer<'a, 'b>,
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

                // Ignore explicit `children` prop when actual JSX children exist.
                // This matches Solid's behavior: JSX children win over `children={...}`.
                if raw_key == "children" && !element.children.is_empty() {
                    continue;
                }

                // Handle ref prop specially - needs ref forwarding
                if raw_key == "ref" {
                    if let Some(JSXAttributeValue::ExpressionContainer(container)) = &attr.value {
                        if let Some(expr) = container.expression.as_expression() {
                            // ref(r$) { var _ref$ = expr; typeof _ref$ === "function" ? _ref$(r$) : expr = r$; }
                            let ref_param = ast.binding_pattern_binding_identifier(
                                span,
                                ast.allocator.alloc_str("r$"),
                            );

                            let params = ast.alloc_formal_parameters(
                                span,
                                FormalParameterKind::FormalParameter,
                                ast.vec1(ast.plain_formal_parameter(span, ref_param)),
                                NONE,
                            );

                            let mut body_stmts = ast.vec_with_capacity(2);
                            let init_expr = context.clone_expr(expr);
                            let var_decl = {
                                let declarator = ast.variable_declarator(
                                    span,
                                    VariableDeclarationKind::Var,
                                    ast.binding_pattern_binding_identifier(
                                        span,
                                        ast.allocator.alloc_str("_ref$"),
                                    ),
                                    NONE,
                                    Some(init_expr),
                                    false,
                                );
                                Statement::VariableDeclaration(ast.alloc_variable_declaration(
                                    span,
                                    VariableDeclarationKind::Var,
                                    ast.vec1(declarator),
                                    false,
                                ))
                            };
                            body_stmts.push(var_decl);

                            let ref_ident = ast.expression_identifier(span, "_ref$");
                            let r_ident = ast.expression_identifier(span, "r$");
                            let typeof_ref = ast.expression_unary(
                                span,
                                UnaryOperator::Typeof,
                                ref_ident.clone_in(ast.allocator),
                            );
                            let function_str = ast.expression_string_literal(
                                span,
                                ast.allocator.alloc_str("function"),
                                None,
                            );
                            let test = ast.expression_binary(
                                span,
                                typeof_ref,
                                BinaryOperator::StrictEquality,
                                function_str,
                            );

                            let call = {
                                let mut args = ast.vec_with_capacity(1);
                                args.push(Argument::from(r_ident.clone_in(ast.allocator)));
                                ast.expression_call(
                                    span,
                                    ref_ident.clone_in(ast.allocator),
                                    None::<oxc_ast::ast::TSTypeParameterInstantiation<'a>>,
                                    args,
                                    false,
                                )
                            };

                            let assign = expression_to_assignment_target(context.clone_expr(expr))
                                .map(|target| {
                                    ast.expression_assignment(
                                        span,
                                        AssignmentOperator::Assign,
                                        target,
                                        r_ident.clone_in(ast.allocator),
                                    )
                                })
                                .unwrap_or_else(|| ast.expression_identifier(SPAN, "undefined"));

                            let conditional = ast.expression_conditional(span, test, call, assign);
                            body_stmts.push(Statement::ExpressionStatement(
                                ast.alloc_expression_statement(span, conditional),
                            ));

                            let body = ast.alloc_function_body(span, ast.vec(), body_stmts);
                            let func = ast.expression_function(
                                span,
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
                            );

                            dynamic_props.push(ast.object_property_kind_object_property(
                                span,
                                PropertyKind::Init,
                                PropertyKey::StaticIdentifier(
                                    ast.alloc_identifier_name(span, "ref"),
                                ),
                                func,
                                true,  // method
                                false, // shorthand
                                false, // computed
                            ));
                        }
                    }
                    continue;
                }

                let key = make_prop_key(ast, attr.span, &raw_key);

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
        if let Some(children) = get_children_expr_transformed(element, context, transform_child) {
            let key = make_prop_key(ast, span, "children");
            if is_dynamic(&children) {
                dynamic_props.push(ast.object_property_kind_object_property(
                    span,
                    PropertyKind::Get,
                    key,
                    getter_return_expr(ast, span, children),
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
    }

    let has_inline_props = !static_props.is_empty() || !dynamic_props.is_empty();
    let mut props = ast.vec_with_capacity(static_props.len() + dynamic_props.len());
    for prop in static_props {
        props.push(prop);
    }
    for prop in dynamic_props {
        props.push(prop);
    }

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
