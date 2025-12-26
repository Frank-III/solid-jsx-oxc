//! Main SSR transform logic
//!
//! This implements the Traverse trait to walk the AST and transform JSX for SSR.

use oxc_allocator::{Allocator, CloneIn};
use oxc_ast::ast::{
    Expression, JSXElement, JSXFragment, JSXChild, JSXExpressionContainer,
    JSXText, Program, Statement, ImportOrExportKind, ModuleExportName,
    ImportDeclarationSpecifier, TemplateElementValue,
};
use oxc_span::{Span, SourceType};
use oxc_traverse::{Traverse, TraverseCtx, traverse_mut};
use oxc_semantic::SemanticBuilder;
use oxc_parser::Parser;

use common::{TransformOptions, is_component, get_tag_name};

use crate::ir::{SSRContext, SSRResult};
use crate::element::transform_element;
use crate::component::transform_component;

/// The main SSR JSX transformer
pub struct SSRTransform<'a> {
    allocator: &'a Allocator,
    options: &'a TransformOptions<'a>,
    context: SSRContext,
}

impl<'a> SSRTransform<'a> {
    pub fn new(allocator: &'a Allocator, options: &'a TransformOptions<'a>) -> Self {
        Self {
            allocator,
            options,
            context: SSRContext::new(options.hydratable),
        }
    }

    /// Run the transform on a program
    pub fn transform(mut self, program: &mut Program<'a>) {
        // SAFETY: We convert the allocator reference to a raw pointer and back to a reference
        // to satisfy oxc_traverse's API which requires `&Allocator` while we hold `&mut self`.
        // This is safe because:
        // 1. The allocator lives for 'a which outlives this entire transform operation
        // 2. oxc_traverse only uses the allocator for read-only arena access
        // 3. We don't mutate the allocator through any path during traversal
        // 4. The pointer is never escaped or stored beyond this call
        let allocator = self.allocator as *const Allocator;
        traverse_mut(
            &mut self,
            unsafe { &*allocator },
            program,
            SemanticBuilder::new()
                .build(program)
                .semantic
                .into_scoping(),
            (),
        );
    }

    /// Transform a JSX node and return the SSR result
    fn transform_node(
        &self,
        node: &JSXChild<'a>,
    ) -> Option<SSRResult> {
        match node {
            JSXChild::Element(element) => {
                Some(self.transform_jsx_element(element))
            }
            JSXChild::Fragment(fragment) => {
                Some(self.transform_fragment(fragment))
            }
            JSXChild::Text(text) => {
                self.transform_text(text)
            }
            JSXChild::ExpressionContainer(container) => {
                self.transform_expression_container(container)
            }
            JSXChild::Spread(_) => {
                // Spread children - treat as dynamic
                let mut result = SSRResult::new();
                self.context.register_helper("escape");
                result.push_dynamic("/* spread */".to_string(), false, false);
                Some(result)
            }
        }
    }

    /// Transform a JSX element
    fn transform_jsx_element(
        &self,
        element: &JSXElement<'a>,
    ) -> SSRResult {
        let tag_name = get_tag_name(element);

        if is_component(&tag_name) {
            // Create child transformer closure that can recursively transform children
            let child_transformer = |child: &JSXChild<'a>| -> Option<SSRResult> {
                self.transform_node(child)
            };
            transform_component(element, &tag_name, &self.context, self.options, &child_transformer)
        } else {
            transform_element(element, &tag_name, &self.context, self.options)
        }
    }

    /// Transform a JSX fragment
    fn transform_fragment(
        &self,
        fragment: &JSXFragment<'a>,
    ) -> SSRResult {
        let mut result = SSRResult::new();

        for child in &fragment.children {
            if let Some(child_result) = self.transform_node(child) {
                result.merge(child_result);
            }
        }

        result
    }

    /// Transform JSX text
    fn transform_text(&self, text: &JSXText<'a>) -> Option<SSRResult> {
        let content = common::expression::trim_whitespace(&text.value);
        if content.is_empty() {
            return None;
        }

        let mut result = SSRResult::new();
        result.push_static(&common::expression::escape_html(&content, false));
        Some(result)
    }

    /// Transform a JSX expression container
    fn transform_expression_container(
        &self,
        container: &JSXExpressionContainer<'a>,
    ) -> Option<SSRResult> {
        if let Some(_expr) = container.expression.as_expression() {
            self.context.register_helper("escape");
            let mut result = SSRResult::new();
            result.push_dynamic("/* expr */".to_string(), false, false);
            Some(result)
        } else {
            None
        }
    }
}

impl<'a> Traverse<'a, ()> for SSRTransform<'a> {
    // Use exit_expression instead of enter_expression to avoid
    // oxc_traverse walking into our newly created nodes (which lack scope info)
    fn exit_expression(
        &mut self,
        node: &mut Expression<'a>,
        ctx: &mut TraverseCtx<'a, ()>,
    ) {
        let new_expr = match node {
            Expression::JSXElement(element) => {
                let result = self.transform_jsx_element(element);
                Some(self.build_ssr_expression(&result, ctx))
            }
            Expression::JSXFragment(fragment) => {
                let result = self.transform_fragment(fragment);
                Some(self.build_ssr_expression(&result, ctx))
            }
            _ => None,
        };

        if let Some(expr) = new_expr {
            *node = expr;
        }
    }

    fn exit_program(&mut self, program: &mut Program<'a>, ctx: &mut TraverseCtx<'a, ()>) {
        // Get the helpers that were used
        let helpers = self.context.helpers.borrow();

        if helpers.is_empty() {
            return;
        }

        // Build import statement: import { ssr, escape, ... } from 'solid-js/web';
        // NOTE: This import building logic is duplicated with DOM transform.
        // Extraction is non-trivial due to OXC's lifetime requirements.
        let ast = ctx.ast;
        let span = Span::default();
        let module_name = self.options.module_name;

        // Build specifiers
        let mut specifiers = ast.vec();
        for helper in helpers.iter() {
            let helper_str = ast.allocator.alloc_str(helper);
            let imported = ModuleExportName::IdentifierName(
                ast.identifier_name(span, helper_str)
            );
            let local = ast.binding_identifier(span, helper_str);
            let specifier = ast.import_specifier(
                span,
                imported,
                local,
                ImportOrExportKind::Value,
            );
            specifiers.push(ImportDeclarationSpecifier::ImportSpecifier(
                ast.alloc(specifier)
            ));
        }

        // Build source string literal
        let source = ast.string_literal(span, module_name, None);

        // Build import declaration
        let import_decl = ast.import_declaration(
            span,
            Some(specifiers),
            source,
            None, // phase
            None::<oxc_ast::ast::WithClause<'a>>, // with_clause
            ImportOrExportKind::Value,
        );

        // Create the statement
        let import_stmt = Statement::ImportDeclaration(ast.alloc(import_decl));

        // Insert at the beginning of the program
        program.body.insert(0, import_stmt);
    }
}

impl<'a> SSRTransform<'a> {
    /// Build the SSR expression from the transform result
    fn build_ssr_expression(
        &self,
        result: &SSRResult,
        ctx: &mut TraverseCtx<'a, ()>,
    ) -> Expression<'a> {
        let ast = ctx.ast;
        let span = Span::default();

        // If no dynamic values, just return a string literal
        if result.template_values.is_empty() {
            let content = result.template_parts.join("");
            let allocated_str = ast.allocator.alloc_str(&content);
            return ast.expression_string_literal(span, allocated_str, None);
        }

        // Build a proper TaggedTemplateExpression: ssr`...${expr}...`
        self.context.register_helper("ssr");

        // Build quasis (static template parts)
        let mut quasis = ast.vec();
        for (i, part) in result.template_parts.iter().enumerate() {
            let is_tail = i == result.template_parts.len() - 1;
            let part_str = ast.allocator.alloc_str(part);
            let value = TemplateElementValue {
                raw: ast.atom(part_str),
                cooked: Some(ast.atom(part_str)),
            };
            let element = ast.template_element(span, value, is_tail);
            quasis.push(element);
        }

        // Build expressions (dynamic parts)
        let mut expressions = ast.vec();
        for val in &result.template_values {
            let expr = self.parse_and_wrap_expression(&val.expr, val.is_attr, val.skip_escape, ctx);
            expressions.push(expr);
        }

        // Build the template literal
        let template = ast.template_literal(span, quasis, expressions);

        // Build the tag (ssr identifier)
        let tag = ast.expression_identifier(span, "ssr");

        // Build the tagged template expression
        // Args: span, tag, type_arguments, quasi (template)
        ast.expression_tagged_template(
            span,
            tag,
            None::<oxc_ast::ast::TSTypeParameterInstantiation<'a>>,
            template,
        )
    }

    /// Parse an expression string and wrap it appropriately
    fn parse_and_wrap_expression(
        &self,
        expr_str: &str,
        is_attr: bool,
        skip_escape: bool,
        ctx: &mut TraverseCtx<'a, ()>,
    ) -> Expression<'a> {
        let ast = ctx.ast;
        let span = Span::default();

        // Try to parse the expression
        let parsed_expr = self.parse_expression(expr_str, ctx);

        if skip_escape {
            // Don't wrap in escape()
            parsed_expr
        } else if is_attr {
            // Wrap in escape(expr, true)
            self.build_escape_call(parsed_expr, true, ctx)
        } else {
            // Wrap in escape(expr)
            self.build_escape_call(parsed_expr, false, ctx)
        }
    }

    /// Parse an expression string into an AST Expression
    fn parse_expression(
        &self,
        expr_str: &str,
        ctx: &mut TraverseCtx<'a, ()>,
    ) -> Expression<'a> {
        let ast = ctx.ast;
        let span = Span::default();

        // Use the arena allocator to parse the expression
        let allocator = ast.allocator;

        // Parse the expression string
        let source_type = SourceType::tsx();
        let parse_result = Parser::new(allocator, expr_str, source_type).parse();

        // Try to extract the expression from the parsed program
        if let Some(stmt) = parse_result.program.body.first() {
            if let Statement::ExpressionStatement(expr_stmt) = stmt {
                // Clone the expression into our allocator
                // Note: This is a simplified approach - ideally we'd transfer ownership
                return expr_stmt.expression.clone_in(allocator);
            }
        }

        // Fallback: create an identifier from the expression string
        // This handles simple cases like variable names
        let expr_alloc = ast.allocator.alloc_str(expr_str);
        ast.expression_identifier(span, expr_alloc)
    }

    /// Build an escape() call expression
    fn build_escape_call(
        &self,
        expr: Expression<'a>,
        is_attr: bool,
        ctx: &mut TraverseCtx<'a, ()>,
    ) -> Expression<'a> {
        let ast = ctx.ast;
        let span = Span::default();

        // Create: escape(expr) or escape(expr, true)
        let callee = ast.expression_identifier(span, "escape");

        let mut args = ast.vec();

        // First argument: the expression
        args.push(oxc_ast::ast::Argument::from(expr));

        if is_attr {
            // Second argument: true (for attribute escaping)
            let true_lit = ast.expression_boolean_literal(span, true);
            args.push(oxc_ast::ast::Argument::from(true_lit));
        }

        ast.expression_call(
            span,
            callee,
            None::<oxc_ast::ast::TSTypeParameterInstantiation<'a>>,
            args,
            false,
        )
    }
}
