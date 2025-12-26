//! Main JSX transform logic
//! This implements the Traverse trait to walk the AST and transform JSX

use oxc_allocator::{Allocator, CloneIn};
use oxc_ast::ast::{
    Expression, JSXElement, JSXFragment, JSXChild, JSXExpressionContainer,
    JSXText, Program, Statement, ImportOrExportKind, ModuleExportName,
    ImportDeclarationSpecifier,
};
use oxc_span::{Span, SourceType};
use oxc_traverse::{Traverse, TraverseCtx, traverse_mut};
use oxc_semantic::SemanticBuilder;
use oxc_parser::Parser;

use common::{TransformOptions, is_component, get_tag_name, expr_to_string};

use crate::ir::{BlockContext, TransformResult};
use crate::element::transform_element;
use crate::component::transform_component;

/// The main Solid JSX transformer
pub struct SolidTransform<'a> {
    allocator: &'a Allocator,
    options: &'a TransformOptions<'a>,
    context: BlockContext,
}

impl<'a> SolidTransform<'a> {
    pub fn new(allocator: &'a Allocator, options: &'a TransformOptions<'a>) -> Self {
        Self {
            allocator,
            options,
            context: BlockContext::new(),
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
    ) -> Option<TransformResult> {
        match node {
            JSXChild::Element(element) => {
                Some(self.transform_jsx_element(element, info))
            }
            JSXChild::Fragment(fragment) => {
                Some(self.transform_fragment(fragment, info))
            }
            JSXChild::Text(text) => {
                self.transform_text(text)
            }
            JSXChild::ExpressionContainer(container) => {
                self.transform_expression_container(container, info)
            }
            JSXChild::Spread(_spread) => {
                // Spread children are rare, treat as dynamic
                Some(TransformResult {
                    exprs: vec![crate::ir::Expr {
                        code: format!("/* spread child */"),
                    }],
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
    ) -> TransformResult {
        let tag_name = get_tag_name(element);

        if is_component(&tag_name) {
            // Create child transformer closure that can recursively transform children
            let child_transformer = |child: &JSXChild<'a>| -> Option<TransformResult> {
                self.transform_node(child, info)
            };
            transform_component(element, &tag_name, &self.context, self.options, &child_transformer)
        } else {
            transform_element(element, &tag_name, info, &self.context, self.options)
        }
    }

    /// Transform a JSX fragment
    fn transform_fragment(
        &self,
        fragment: &JSXFragment<'a>,
        info: &TransformInfo,
    ) -> TransformResult {
        let mut result = TransformResult::default();

        for child in &fragment.children {
            if let Some(child_result) = self.transform_node(child, info) {
                // Merge child results
                result.template.push_str(&child_result.template);
                result.declarations.extend(child_result.declarations);
                result.exprs.extend(child_result.exprs);
                result.dynamics.extend(child_result.dynamics);
            }
        }

        result
    }

    /// Transform JSX text
    fn transform_text(&self, text: &JSXText<'a>) -> Option<TransformResult> {
        let content = common::expression::trim_whitespace(&text.value);
        if content.is_empty() {
            return None;
        }

        Some(TransformResult {
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
    ) -> Option<TransformResult> {
        // Use as_expression() to get the expression if it exists
        if let Some(expr) = container.expression.as_expression() {
            let expr_str = expr_to_string(expr);
            if common::is_dynamic(expr) {
                // Wrap in arrow function for reactivity
                Some(TransformResult {
                    exprs: vec![crate::ir::Expr {
                        code: format!("() => {}", expr_str),
                    }],
                    ..Default::default()
                })
            } else {
                // Static expression
                Some(TransformResult {
                    exprs: vec![crate::ir::Expr {
                        code: expr_str,
                    }],
                    ..Default::default()
                })
            }
        } else {
            // Empty expression
            None
        }
    }

    /// Build DOM output code from transform result
    fn build_dom_output(&self, result: &TransformResult) -> String {
        let mut code = String::new();

        // If there's a template, we need to clone it
        if !result.template.is_empty() && !result.skip_template {
            // Register template helper
            self.context.register_helper("template");

            // Push template and get variable name
            let tmpl_idx = self.context.push_template(result.template.clone(), result.is_svg);
            let tmpl_var = format!("_tmpl${}", tmpl_idx + 1);

            // Generate element variable
            let elem_var = result.id.clone().unwrap_or_else(|| "_el$".to_string());

            // Build IIFE
            code.push_str("(() => {\n");
            code.push_str(&format!("  const {} = {}.cloneNode(true);\n", elem_var, tmpl_var));

            // Add declarations (element walking for nested elements)
            for decl in &result.declarations {
                code.push_str(&format!("  const {} = {};\n", decl.name, decl.init));
            }

            // Add expressions (effects, inserts, etc.)
            for expr in &result.exprs {
                code.push_str(&format!("  {};\n", expr.code));
            }

            // Add dynamic bindings
            for binding in &result.dynamics {
                self.context.register_helper("effect");
                self.context.register_helper("setAttribute");
                code.push_str(&format!(
                    "  effect(() => setAttribute({}, \"{}\", {}));\n",
                    binding.elem, binding.key, binding.value
                ));
            }

            code.push_str(&format!("  return {};\n", elem_var));
            code.push_str("})()");
        } else if !result.exprs.is_empty() {
            // Just expressions (like a component call)
            code = result.exprs.iter()
                .map(|e| e.code.clone())
                .collect::<Vec<_>>()
                .join(", ");
        }

        code
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
    fn exit_expression(
        &mut self,
        node: &mut Expression<'a>,
        ctx: &mut TraverseCtx<'a, ()>,
    ) {
        let new_expr = match node {
            Expression::JSXElement(element) => {
                let result = self.transform_jsx_element(element, &TransformInfo {
                    top_level: true,
                    last_element: true,
                    ..Default::default()
                });
                Some(self.build_dom_expression(&result, ctx))
            }
            Expression::JSXFragment(fragment) => {
                let result = self.transform_fragment(fragment, &TransformInfo {
                    top_level: true,
                    ..Default::default()
                });
                Some(self.build_dom_expression(&result, ctx))
            }
            _ => None,
        };

        if let Some(expr) = new_expr {
            *node = expr;
        }
    }

    fn exit_program(&mut self, program: &mut Program<'a>, ctx: &mut TraverseCtx<'a, ()>) {
        let helpers = self.context.helpers.borrow();
        let templates = self.context.templates.borrow();
        let delegates = self.context.delegates.borrow();

        if helpers.is_empty() && templates.is_empty() {
            return;
        }

        let ast = ctx.ast;
        let span = Span::default();

        // Insert template declarations
        // const _tmpl$ = template(`<div></div>`);
        for (i, tmpl) in templates.iter().enumerate() {
            let tmpl_var = format!("_tmpl${}", i + 1);
            let call_code = if tmpl.is_svg {
                format!("template(`{}`, true)", tmpl.content)
            } else {
                format!("template(`{}`)", tmpl.content)
            };

            // Parse and build the declaration
            let decl_code = format!("const {} = {};", tmpl_var, call_code);
            if let Some(stmt) = self.parse_statement(&decl_code, ctx) {
                program.body.insert(0, stmt);
            }
        }

        // Insert delegateEvents call if needed
        if !delegates.is_empty() {
            let events: Vec<&str> = delegates.iter().map(|s| s.as_str()).collect();
            let delegate_code = format!("delegateEvents([\"{}\"])", events.join("\", \""));
            if let Some(stmt) = self.parse_statement(&format!("{};", delegate_code), ctx) {
                program.body.push(stmt);
            }
            // Register helper
            drop(helpers); // Release borrow
            self.context.register_helper("delegateEvents");
        }

        // Re-borrow helpers after potential modification
        let helpers = self.context.helpers.borrow();

        // Build import statement: import { template, effect, ... } from 'solid-js/web';
        // NOTE: This import building logic is duplicated with SSR transform.
        // Extraction is non-trivial due to OXC's lifetime requirements.
        if !helpers.is_empty() {
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
}

impl<'a> SolidTransform<'a> {
    /// Build DOM expression from transform result
    fn build_dom_expression(
        &self,
        result: &TransformResult,
        ctx: &mut TraverseCtx<'a, ()>,
    ) -> Expression<'a> {
        let ast = ctx.ast;
        let span = Span::default();

        // Generate the DOM code string
        let dom_code = self.build_dom_output(result);

        // Parse the code into an expression
        let allocator = ast.allocator;
        let source_type = SourceType::tsx();
        let parse_result = Parser::new(allocator, &dom_code, source_type).parse();

        // Try to extract the expression from the parsed program
        if let Some(stmt) = parse_result.program.body.first() {
            if let Statement::ExpressionStatement(expr_stmt) = stmt {
                return expr_stmt.expression.clone_in(allocator);
            }
        }

        // Fallback: create a string literal with the code (for debugging)
        let code_str = ast.allocator.alloc_str(&dom_code);
        ast.expression_string_literal(span, code_str, None)
    }

    /// Parse a statement string into a Statement
    fn parse_statement(
        &self,
        code: &str,
        ctx: &mut TraverseCtx<'a, ()>,
    ) -> Option<Statement<'a>> {
        let ast = ctx.ast;
        let allocator = ast.allocator;
        let source_type = SourceType::tsx();
        let parse_result = Parser::new(allocator, code, source_type).parse();

        parse_result.program.body.first()
            .map(|stmt| stmt.clone_in(allocator))
    }
}
