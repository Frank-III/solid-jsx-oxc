use oxc_allocator::CloneIn;
use oxc_ast::ast::{
    Argument, ArrayExpressionElement, Expression, FormalParameterKind, Statement,
    VariableDeclarationKind,
};
use oxc_ast::{AstBuilder, NONE};
use oxc_span::{Span, SPAN};

use crate::ir::{BlockContext, TransformResult};

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

fn const_decl_stmt<'a>(
    ast: AstBuilder<'a>,
    span: Span,
    name: &str,
    init: Expression<'a>,
) -> Statement<'a> {
    let declarator = ast.variable_declarator(
        span,
        VariableDeclarationKind::Const,
        ast.binding_pattern_binding_identifier(span, ast.allocator.alloc_str(name)),
        NONE,
        Some(init),
        false,
    );
    Statement::VariableDeclaration(ast.alloc_variable_declaration(
        span,
        VariableDeclarationKind::Const,
        ast.vec1(declarator),
        false,
    ))
}

fn arrow_zero_params_body<'a>(
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
    let mut statements = ast.vec_with_capacity(1);
    statements.push(Statement::ExpressionStatement(
        ast.alloc_expression_statement(span, expr),
    ));
    let body = ast.alloc_function_body(span, ast.vec(), statements);
    ast.expression_arrow_function(span, true, false, NONE, params, NONE, body)
}

pub fn build_dom_output_expr<'a>(
    result: &TransformResult<'a>,
    context: &BlockContext<'a>,
) -> Expression<'a> {
    let ast = context.ast();
    let gen_span = SPAN;

    // Fragment with mixed children (array output)
    if !result.child_results.is_empty() {
        let mut elements = ast.vec_with_capacity(result.child_results.len());
        for child in &result.child_results {
            let expr = build_dom_output_expr(child, context);
            elements.push(ArrayExpressionElement::from(expr));
        }
        return ast.expression_array(gen_span, elements);
    }

    // Text-only result
    if result.text && !result.template.is_empty() {
        return ast.expression_string_literal(
            gen_span,
            ast.allocator.alloc_str(&result.template),
            None,
        );
    }

    // Template-backed result
    if !result.template.is_empty() && !result.skip_template {
        // Push template and get variable name
        // The template string is generated code; don't attribute it to the source with spans.
        let tmpl_idx = context.push_template(result.template.clone(), result.is_svg, gen_span);
        let tmpl_var = format!("_tmpl${}", tmpl_idx + 1);

        // Use the generated element ID when available (matches expression wiring).
        // Fall back to a local _el$ when the element didn't require a stable ID.
        let elem_var = result.id.clone().unwrap_or_else(|| "_el$".to_string());

        let mut statements = ast.vec();

        // const _el$ = _tmpl$1.cloneNode(true);
        let clone_call = call_expr(
            ast,
            gen_span,
            static_member(
                ast,
                gen_span,
                ident_expr(ast, gen_span, &tmpl_var),
                "cloneNode",
            ),
            [ast.expression_boolean_literal(gen_span, true)],
        );
        statements.push(const_decl_stmt(ast, gen_span, &elem_var, clone_call));

        // const child = _el$.firstChild.nextSibling;
        for decl in &result.declarations {
            statements.push(const_decl_stmt(
                ast,
                gen_span,
                &decl.name,
                decl.init.clone_in(ast.allocator),
            ));
        }

        // Expressions (effects, inserts, etc.)
        for expr in &result.exprs {
            statements.push(Statement::ExpressionStatement(
                ast.alloc_expression_statement(gen_span, expr.clone_in(ast.allocator)),
            ));
        }

        // Dynamic bindings (effect(() => setter))
        for binding in &result.dynamics {
            context.register_helper("effect");
            if binding.key == "style" {
                context.register_helper("style");
            } else if binding.key == "classList" {
                context.register_helper("classList");
            } else {
                context.register_helper("setAttribute");
            }

            let setter = crate::template::generate_set_attr_expr(ast, gen_span, binding);
            let effect = ident_expr(ast, gen_span, "effect");
            let arrow = arrow_zero_params_body(ast, gen_span, setter);
            let effect_call = call_expr(ast, gen_span, effect, [arrow]);
            statements.push(Statement::ExpressionStatement(
                ast.alloc_expression_statement(gen_span, effect_call),
            ));
        }

        // Post expressions
        for expr in &result.post_exprs {
            statements.push(Statement::ExpressionStatement(
                ast.alloc_expression_statement(gen_span, expr.clone_in(ast.allocator)),
            ));
        }

        // return _el$;
        statements.push(Statement::ReturnStatement(ast.alloc_return_statement(
            gen_span,
            Some(ident_expr(ast, gen_span, &elem_var)),
        )));

        // (() => { ... })()
        let params = ast.alloc_formal_parameters(
            gen_span,
            FormalParameterKind::ArrowFormalParameters,
            ast.vec(),
            NONE,
        );
        let body = ast.alloc_function_body(gen_span, ast.vec(), statements);
        let arrow_fn =
            ast.expression_arrow_function(gen_span, false, false, NONE, params, NONE, body);
        return call_expr(ast, gen_span, arrow_fn, []);
    }

    // Expression-only result (like createComponent(...) or fragment expression)
    if !result.exprs.is_empty() {
        if result.needs_memo {
            context.register_helper("memo");
            let callee = ident_expr(ast, gen_span, "memo");
            let mut args = ast.vec_with_capacity(result.exprs.len());
            for expr in &result.exprs {
                args.push(Argument::from(expr.clone_in(ast.allocator)));
            }
            return ast.expression_call(
                gen_span,
                callee,
                None::<oxc_ast::ast::TSTypeParameterInstantiation<'a>>,
                args,
                false,
            );
        }

        if result.exprs.len() == 1 {
            return result.exprs[0].clone_in(ast.allocator);
        }

        let mut exprs = ast.vec_with_capacity(result.exprs.len());
        for expr in &result.exprs {
            exprs.push(expr.clone_in(ast.allocator));
        }
        return ast.expression_sequence(gen_span, exprs);
    }

    // Fallback: empty string literal (matches previous parse-fallback behavior for empty output)
    ast.expression_string_literal(gen_span, ast.allocator.alloc_str(""), None)
}
