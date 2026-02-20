//! Main JSX transform logic
//! This implements the Traverse trait to walk the AST and transform JSX

use oxc_allocator::Allocator;
use oxc_ast::ast::{
    Argument, ArrayExpressionElement, Expression, ImportDeclarationSpecifier, ImportOrExportKind,
    JSXChild, JSXElement, JSXExpressionContainer, JSXFragment, JSXText, ModuleExportName, Program,
    Statement, TemplateElementValue, VariableDeclarationKind,
};
use oxc_ast::NONE;
use oxc_semantic::SemanticBuilder;
use oxc_span::SPAN;
use oxc_traverse::{traverse_mut, Traverse, TraverseCtx};

use common::{get_tag_name, is_component, TransformOptions};

use crate::component::transform_component;
use crate::element::transform_element;
use crate::ir::{BlockContext, TransformResult};
use crate::output::build_dom_output_expr;

/// The main Solid JSX transformer
pub struct SolidTransform<'a> {
    allocator: &'a Allocator,
    options: &'a TransformOptions<'a>,
    context: BlockContext<'a>,
}

impl<'a> SolidTransform<'a> {
    pub fn new(allocator: &'a Allocator, options: &'a TransformOptions<'a>) -> Self {
        Self {
            allocator,
            options,
            context: BlockContext::new(allocator),
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

    /// Transform a JSX node and return the result
    fn transform_node(
        &self,
        node: &JSXChild<'a>,
        info: &TransformInfo,
        ctx: &TraverseCtx<'a, ()>,
    ) -> Option<TransformResult<'a>> {
        match node {
            JSXChild::Element(element) => Some(self.transform_jsx_element(element, info, ctx)),
            JSXChild::Fragment(fragment) => Some(self.transform_fragment(fragment, info, ctx)),
            JSXChild::Text(text) => self.transform_text(text),
            JSXChild::ExpressionContainer(container) => {
                self.transform_expression_container(container, info)
            }
            JSXChild::Spread(spread) => {
                // Spread children are rare, treat as dynamic
                let expr = self.context.ast().expression_string_literal(
                    SPAN,
                    self.context.ast().allocator.alloc_str("/* spread child */"),
                    None,
                );
                Some(TransformResult {
                    span: spread.span,
                    exprs: vec![expr],
                    ..Default::default()
                })
            }
        }
    }

    /// Transform a JSX element
    fn transform_jsx_element(
        &self,
        element: &JSXElement<'a>,
        info: &TransformInfo,
        ctx: &TraverseCtx<'a, ()>,
    ) -> TransformResult<'a> {
        let tag_name = get_tag_name(element);

        // Create child transformer closure that can recursively transform children
        let child_transformer = |child: &JSXChild<'a>| -> Option<TransformResult<'a>> {
            self.transform_node(child, info, ctx)
        };

        if is_component(&tag_name) {
            transform_component(
                element,
                &tag_name,
                &self.context,
                self.options,
                &child_transformer,
                ctx,
            )
        } else {
            transform_element(
                element,
                &tag_name,
                info,
                &self.context,
                self.options,
                &child_transformer,
                ctx,
            )
        }
    }

    /// Transform a JSX fragment
    fn transform_fragment(
        &self,
        fragment: &JSXFragment<'a>,
        info: &TransformInfo,
        ctx: &TraverseCtx<'a, ()>,
    ) -> TransformResult<'a> {
        let mut result = TransformResult {
            span: fragment.span,
            ..Default::default()
        };
        let mut has_expression_child = false;
        let mut child_results: Vec<TransformResult<'a>> = Vec::new();

        for child in &fragment.children {
            // Track if we have expression container children (need memo)
            if matches!(child, JSXChild::ExpressionContainer(_)) {
                has_expression_child = true;
            }

            if let Some(child_result) = self.transform_node(child, info, ctx) {
                child_results.push(child_result);
            }
        }

        // Handle different fragment scenarios
        if child_results.is_empty() {
            // Empty fragment
            return result;
        }

        if child_results.len() == 1 {
            let mut single_result = child_results.pop().unwrap();
            // For single expression child, check if we need memo
            if single_result.template.is_empty()
                && !single_result.exprs.is_empty()
                && has_expression_child
            {
                single_result.needs_memo = true;
            }
            return single_result;
        }

        // Multiple children:
        // `template()` only returns the first root node, so fragments with more than one root
        // must be emitted as arrays of child outputs.
        //
        // The only safe merge is for plain text, which can be concatenated into a single
        // string expression.
        let all_text_children = child_results.iter().all(|r| r.text);
        if all_text_children {
            result.text = true;
            for child_result in child_results {
                result.template.push_str(&child_result.template);
            }
        } else {
            result.child_results = child_results;
        }

        result
    }

    /// Transform JSX text
    fn transform_text(&self, text: &JSXText<'a>) -> Option<TransformResult<'a>> {
        let content = common::expression::trim_whitespace(&text.value);
        if content.is_empty() {
            return None;
        }

        Some(TransformResult {
            span: text.span,
            template: common::expression::escape_html(&content, false),
            text: true,
            ..Default::default()
        })
    }

    /// Transform a JSX expression container
    fn transform_expression_container(
        &self,
        container: &JSXExpressionContainer<'a>,
        _info: &TransformInfo,
    ) -> Option<TransformResult<'a>> {
        // Use as_expression() to get the expression if it exists
        if let Some(expr) = container.expression.as_expression() {
            if common::is_dynamic(expr) {
                // Wrap in arrow function for reactivity
                let ast = self.context.ast();
                let span = SPAN;
                let params = ast.alloc_formal_parameters(
                    span,
                    oxc_ast::ast::FormalParameterKind::ArrowFormalParameters,
                    ast.vec(),
                    NONE,
                );
                let mut statements = ast.vec_with_capacity(1);
                statements.push(Statement::ExpressionStatement(
                    ast.alloc_expression_statement(span, self.context.clone_expr(expr)),
                ));
                let body = ast.alloc_function_body(span, ast.vec(), statements);
                let arrow =
                    ast.expression_arrow_function(span, true, false, NONE, params, NONE, body);
                Some(TransformResult {
                    span: container.span,
                    exprs: vec![arrow],
                    ..Default::default()
                })
            } else {
                // Static expression
                Some(TransformResult {
                    span: container.span,
                    exprs: vec![self.context.clone_expr(expr)],
                    ..Default::default()
                })
            }
        } else {
            // Empty expression
            None
        }
    }
}

/// Additional info passed during transform
#[derive(Default, Clone)]
pub struct TransformInfo {
    pub top_level: bool,
    pub last_element: bool,
    pub skip_id: bool,
    pub component_child: bool,
    pub fragment_child: bool,
    /// Path from root element to this element (e.g., ["firstChild", "nextSibling"])
    pub path: Vec<String>,
    /// The root element variable name (e.g., "_el$1")
    pub root_id: Option<String>,
}

impl<'a> Traverse<'a, ()> for SolidTransform<'a> {
    // Use exit_expression instead of enter_expression to avoid
    // oxc_traverse walking into our newly created nodes (which lack scope info)
    fn exit_expression(&mut self, node: &mut Expression<'a>, ctx: &mut TraverseCtx<'a, ()>) {
        let new_expr = match node {
            Expression::JSXElement(element) => {
                let result = self.transform_jsx_element(
                    element,
                    &TransformInfo {
                        top_level: true,
                        last_element: true,
                        ..Default::default()
                    },
                    ctx,
                );
                Some(build_dom_output_expr(&result, &self.context))
            }
            Expression::JSXFragment(fragment) => {
                let result = self.transform_fragment(
                    fragment,
                    &TransformInfo {
                        top_level: true,
                        ..Default::default()
                    },
                    ctx,
                );
                Some(build_dom_output_expr(&result, &self.context))
            }
            _ => None,
        };

        if let Some(expr) = new_expr {
            *node = expr;
        }
    }

    fn exit_program(&mut self, program: &mut Program<'a>, ctx: &mut TraverseCtx<'a, ()>) {
        let templates = self.context.templates.borrow();
        let delegates = self.context.delegates.borrow();
        let has_helpers = !self.context.helpers.borrow().is_empty();

        if !has_helpers && templates.is_empty() && delegates.is_empty() {
            return;
        }

        let ast = ctx.ast;
        let span = SPAN;

        // Insert delegateEvents call if needed
        if !delegates.is_empty() {
            self.context.register_helper("delegateEvents");

            let mut elements = ast.vec_with_capacity(delegates.len());
            for event in delegates.iter() {
                elements.push(ArrayExpressionElement::from(ast.expression_string_literal(
                    span,
                    ast.allocator.alloc_str(event),
                    None,
                )));
            }
            let array = ast.expression_array(span, elements);
            let callee = ast.expression_identifier(span, "delegateEvents");
            let call = ast.expression_call(
                span,
                callee,
                None::<oxc_ast::ast::TSTypeParameterInstantiation<'a>>,
                ast.vec1(Argument::from(array)),
                false,
            );
            program.body.push(Statement::ExpressionStatement(
                ast.alloc_expression_statement(span, call),
            ));
        }

        let helpers = self.context.helpers.borrow();

        let mut prepend = Vec::new();

        // Build import statement: import { template, effect, ... } from 'solid-js/web';
        // NOTE: This import building logic is duplicated with SSR transform.
        // Extraction is non-trivial due to OXC's lifetime requirements.
        if !helpers.is_empty() {
            let module_name = self.options.module_name;

            // Avoid duplicating helper imports by checking for existing local bindings.
            // We check ALL imports (not just from module_name) because helpers like
            // `mergeProps` can be imported from either `solid-js` or `solid-js/web`.
            let mut existing_helper_locals = std::collections::HashSet::<String>::new();
            let mut first_augmentable_module_import_index: Option<usize> = None;
            for (i, stmt) in program.body.iter().enumerate() {
                let Statement::ImportDeclaration(import_decl) = stmt else {
                    continue;
                };
                if import_decl.import_kind != ImportOrExportKind::Value {
                    continue;
                }

                let is_target_module = import_decl.source.value.as_str() == module_name;

                // Track first import from target module that can be legally augmented.
                // Namespace imports cannot be combined with named imports:
                // `import * as Solid, { template } from "...";` is invalid JS.
                if is_target_module && first_augmentable_module_import_index.is_none() {
                    if let Some(specifiers) = &import_decl.specifiers {
                        let has_namespace_specifier = specifiers.iter().any(|spec| {
                            matches!(
                                spec,
                                ImportDeclarationSpecifier::ImportNamespaceSpecifier(_)
                            )
                        });
                        if !has_namespace_specifier {
                            first_augmentable_module_import_index = Some(i);
                        }
                    }
                }

                // Collect ALL import bindings to avoid duplicate declarations
                if let Some(specifiers) = &import_decl.specifiers {
                    for spec in specifiers.iter() {
                        match spec {
                            ImportDeclarationSpecifier::ImportSpecifier(s) => {
                                existing_helper_locals.insert(s.local.name.as_str().to_string());
                            }
                            ImportDeclarationSpecifier::ImportDefaultSpecifier(s) => {
                                existing_helper_locals.insert(s.local.name.as_str().to_string());
                            }
                            ImportDeclarationSpecifier::ImportNamespaceSpecifier(s) => {
                                existing_helper_locals.insert(s.local.name.as_str().to_string());
                            }
                        }
                    }
                }
            }

            // Build specifiers
            let mut specifiers = ast.vec();
            for helper in helpers.iter().filter(|h| !existing_helper_locals.contains(*h)) {
                let helper_str = ast.allocator.alloc_str(helper);
                let imported =
                    ModuleExportName::IdentifierName(ast.identifier_name(span, helper_str));
                let local = ast.binding_identifier(span, helper_str);
                let specifier =
                    ast.import_specifier(span, imported, local, ImportOrExportKind::Value);
                specifiers.push(ImportDeclarationSpecifier::ImportSpecifier(
                    ast.alloc(specifier),
                ));
            }

            if !specifiers.is_empty() {
                // Prefer augmenting the first existing import from the module to avoid extra imports.
                if let Some(import_index) = first_augmentable_module_import_index {
                    if let Statement::ImportDeclaration(import_decl) = &mut program.body[import_index]
                    {
                        let decl_specifiers =
                            import_decl.specifiers.get_or_insert_with(|| ast.vec());
                        decl_specifiers.extend(specifiers);
                    } else {
                        debug_assert!(false, "stored import index should still be an import");
                    }
                } else {
                    // Build source string literal
                    let source = ast.string_literal(span, module_name, None);

                    // Build import declaration
                    let import_decl = ast.import_declaration(
                        span,
                        Some(specifiers),
                        source,
                        None,                                 // phase
                        None::<oxc_ast::ast::WithClause<'a>>, // with_clause
                        ImportOrExportKind::Value,
                    );

                    // Create the statement
                    let import_stmt = Statement::ImportDeclaration(ast.alloc(import_decl));

                    prepend.push(import_stmt);
                }
            }
        }

        // Insert template declarations
        // const _tmpl$1 = template(`<div></div>`);
        for (i, tmpl) in templates.iter().enumerate() {
            let tmpl_span = tmpl.span;
            let tmpl_var = format!("_tmpl${}", i + 1);

            let mut quasis = ast.vec_with_capacity(1);
            let part_str = ast.allocator.alloc_str(&tmpl.content);
            let value = TemplateElementValue {
                raw: ast.atom(part_str),
                cooked: Some(ast.atom(part_str)),
            };
            quasis.push(ast.template_element(tmpl_span, value, true, true));
            let template_lit = ast.template_literal(tmpl_span, quasis, ast.vec());
            let template_expr = Expression::TemplateLiteral(ast.alloc(template_lit));

            let mut args = ast.vec_with_capacity(if tmpl.is_svg { 2 } else { 1 });
            args.push(Argument::from(template_expr));
            if tmpl.is_svg {
                args.push(Argument::from(
                    ast.expression_boolean_literal(tmpl_span, true),
                ));
            }

            let call = ast.expression_call(
                tmpl_span,
                ast.expression_identifier(tmpl_span, "template"),
                None::<oxc_ast::ast::TSTypeParameterInstantiation<'a>>,
                args,
                false,
            );

            let declarator = ast.variable_declarator(
                tmpl_span,
                VariableDeclarationKind::Const,
                ast.binding_pattern_binding_identifier(
                    tmpl_span,
                    ast.allocator.alloc_str(&tmpl_var),
                ),
                NONE,
                Some(call),
                false,
            );

            prepend.push(Statement::VariableDeclaration(
                ast.alloc_variable_declaration(
                    tmpl_span,
                    VariableDeclarationKind::Const,
                    ast.vec1(declarator),
                    false,
                ),
            ));
        }

        // Prepend statements in correct order
        for stmt in prepend.into_iter().rev() {
            program.body.insert(0, stmt);
        }
    }
}
