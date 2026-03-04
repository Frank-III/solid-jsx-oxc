use oxc_allocator::CloneIn;
use oxc_ast::ast::{Argument, AssignmentTarget, Expression, FormalParameterKind, Statement};
use oxc_ast::{AstBuilder, NONE};
use oxc_span::Span;
use oxc_syntax::operator::{AssignmentOperator, LogicalOperator};

use crate::ir::DynamicBinding;

fn ident_expr<'a>(ast: AstBuilder<'a>, span: Span, name: &str) -> Expression<'a> {
    ast.expression_identifier(span, ast.allocator.alloc_str(name))
}

fn static_member<'a>(
    ast: AstBuilder<'a>,
    span: Span,
    object: Expression<'a>,
    property: &str,
) -> Expression<'a> {
    let prop = ast.identifier_name(span, ast.allocator.alloc_str(property));
    Expression::StaticMemberExpression(
        ast.alloc_static_member_expression(span, object, prop, false),
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

fn call_expr<'a>(
    ast: AstBuilder<'a>,
    span: Span,
    callee: Expression<'a>,
    args: impl IntoIterator<Item = Expression<'a>>,
) -> Expression<'a> {
    let mut arguments = ast.vec();
    for arg in args {
        arguments.push(Argument::from(arg));
    }
    ast.expression_call(
        span,
        callee,
        None::<oxc_ast::ast::TSTypeParameterInstantiation<'a>>,
        arguments,
        false,
    )
}

fn arrow_zero_params_expr<'a>(
    ast: AstBuilder<'a>,
    span: Span,
    expr: Expression<'a>,
) -> Expression<'a> {
    let params = ast.alloc_formal_parameters(
        span,
        FormalParameterKind::ArrowFormalParameters,
        ast.vec(),
        NONE,
    );
    let body = ast.alloc_function_body(
        span,
        ast.vec(),
        ast.vec1(Statement::ExpressionStatement(
            ast.alloc_expression_statement(span, expr),
        )),
    );
    ast.expression_arrow_function(span, true, false, NONE, params, NONE, body)
}

pub fn generate_set_attr_expr<'a>(
    ast: AstBuilder<'a>,
    span: Span,
    binding: &DynamicBinding<'a>,
    value: Expression<'a>,
    prev_value: Option<Expression<'a>>,
) -> Expression<'a> {
    let key = binding.key.as_str();
    let elem = ident_expr(ast, span, &binding.elem);

    // Handle special cases
    if key == "class" {
        let callee = ident_expr(ast, span, "className");
        return if let Some(prev) = prev_value {
            let is_svg = ast.expression_boolean_literal(span, binding.is_svg);
            call_expr(
                ast,
                span,
                callee,
                [
                    elem,
                    value,
                    is_svg,
                    prev,
                ],
            )
        } else if binding.is_svg {
            let is_svg = ast.expression_boolean_literal(span, true);
            call_expr(ast, span, callee, [elem, value, is_svg])
        } else {
            call_expr(ast, span, callee, [elem, value])
        };
    }

    if key == "style" {
        let callee = ident_expr(ast, span, "style");
        return if let Some(prev) = prev_value {
            call_expr(ast, span, callee, [elem, value, prev])
        } else {
            call_expr(ast, span, callee, [elem, value])
        };
    }

    if key == "textContent" || key == "innerText" {
        let member = static_member(ast, span, elem, "data");
        if let Some(target) = expression_to_assignment_target(member) {
            return ast.expression_assignment(span, AssignmentOperator::Assign, target, value);
        }
        return ast.expression_identifier(span, "undefined");
    }

    if common::constants::PROPERTIES.contains(key) {
        let member = static_member(ast, span, elem, key);
        if let Some(target) = expression_to_assignment_target(member) {
            let assignment = ast.expression_assignment(span, AssignmentOperator::Assign, target, value);
            if key == "value" && binding.tag_name == "select" {
                let queue_microtask = ident_expr(ast, span, "queueMicrotask");
                let queue_call = call_expr(
                    ast,
                    span,
                    queue_microtask,
                    [arrow_zero_params_expr(
                        ast,
                        span,
                        assignment.clone_in(ast.allocator),
                    )],
                );
                return ast.expression_logical(span, queue_call, LogicalOperator::Or, assignment);
            }
            return assignment;
        }
        return ast.expression_identifier(span, "undefined");
    }

    let set_attr = ident_expr(ast, span, "setAttribute");
    let name = ast.expression_string_literal(span, ast.allocator.alloc_str(key), None);
    call_expr(ast, span, set_attr, [elem, name, value])
}
