use oxc_allocator::CloneIn;
use oxc_ast::ast::{AssignmentTarget, Expression};
use oxc_ast::AstBuilder;
use oxc_span::Span;
use oxc_syntax::operator::AssignmentOperator;

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

pub fn generate_set_attr_expr<'a>(
    ast: AstBuilder<'a>,
    span: Span,
    binding: &DynamicBinding<'a>,
) -> Expression<'a> {
    let key = binding.key.as_str();
    let elem = ident_expr(ast, span, &binding.elem);
    let value = binding.value.clone_in(ast.allocator);

    // Handle special cases
    if key == "class" {
        let callee = ident_expr(ast, span, "className");
        let args = if binding.is_svg {
            let is_svg = ast.expression_boolean_literal(span, true);
            ast.vec_from_array([elem.into(), value.into(), is_svg.into()])
        } else {
            ast.vec_from_array([elem.into(), value.into()])
        };
        return ast.expression_call(
            span,
            callee,
            None::<oxc_ast::ast::TSTypeParameterInstantiation<'a>>,
            args,
            false,
        );
    }

    if key == "style" {
        let callee = ident_expr(ast, span, "style");
        return ast.expression_call(
            span,
            callee,
            None::<oxc_ast::ast::TSTypeParameterInstantiation<'a>>,
            ast.vec_from_array([elem.into(), value.into()]),
            false,
        );
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
            return ast.expression_assignment(span, AssignmentOperator::Assign, target, value);
        }
        return ast.expression_identifier(span, "undefined");
    }

    let set_attr = static_member(ast, span, elem, "setAttribute");
    let name = ast.expression_string_literal(span, ast.allocator.alloc_str(key), None);
    ast.expression_call(
        span,
        set_attr,
        None::<oxc_ast::ast::TSTypeParameterInstantiation<'a>>,
        ast.vec_from_array([name.into(), value.into()]),
        false,
    )
}
