//! Main SSR transform logic
//!
//! This implements the Traverse trait to walk the AST and transform JSX for SSR.

use oxc_allocator::Allocator;
use oxc_ast::ast::{
    ArrayExpressionElement, Expression, ImportDeclarationSpecifier, ImportOrExportKind, JSXChild,
    JSXElement, JSXExpressionContainer, JSXFragment, JSXText, ModuleExportName, Program, Statement,
};
use oxc_semantic::SemanticBuilder;
use oxc_span::SPAN;
use oxc_traverse::{traverse_mut, Traverse, TraverseCtx};

use common::{get_tag_name, is_component, TransformOptions};

use crate::component::transform_component;
use crate::element::transform_element;
use crate::ir::{SSRContext, SSRResult};

/// The main SSR JSX transformer
pub struct SSRTransform<'a> {
    allocator: &'a Allocator,
    options: &'a TransformOptions<'a>,
    context: SSRContext<'a>,
}

impl<'a> SSRTransform<'a> {
    pub fn new(allocator: &'a Allocator, options: &'a TransformOptions<'a>) -> Self {
        Self {
            allocator,
            options,
            context: SSRContext::new(allocator, options.hydratable),
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
    fn transform_node(&self, node: &JSXChild<'a>) -> Option<SSRResult<'a>> {
        match node {
            JSXChild::Element(element) => Some(self.transform_jsx_element(element)),
            JSXChild::Fragment(fragment) => Some(self.transform_fragment(fragment)),
            JSXChild::Text(text) => self.transform_text(text),
            JSXChild::ExpressionContainer(container) => {
                self.transform_expression_container(container)
            }
            JSXChild::Spread(spread) => {
                // Spread children - treat as dynamic
                let mut result = SSRResult::new();
                result.span = spread.span;
                self.context.register_helper("escape");
                result.push_dynamic(self.context.clone_expr(&spread.expression), false, false);
                Some(result)
            }
        }
    }

    /// Transform a JSX element
    fn transform_jsx_element(&self, element: &JSXElement<'a>) -> SSRResult<'a> {
        let tag_name = get_tag_name(element);

        if is_component(&tag_name) {
            // Create child transformer closure that can recursively transform children
            let child_transformer =
                |child: &JSXChild<'a>| -> Option<SSRResult<'a>> { self.transform_node(child) };
            transform_component(
                element,
                &tag_name,
                &self.context,
                self.options,
                &child_transformer,
            )
        } else {
            transform_element(element, &tag_name, &self.context, self.options)
        }
    }

    /// Transform a JSX fragment
    ///
    /// Mirrors Babel's `transformFragmentChildren` (in
    /// `babel-plugin-jsx-dom-expressions/src/shared/fragment.js`): each child
    /// is reduced to its own JS expression and the children are combined as
    /// either a single expression (one child) or a JS *array literal* (two or
    /// more children). The resulting expression replaces the JSXFragment
    /// in-place.
    ///
    /// This is the load-bearing path for top-level fragments like
    ///
    ///   function AppRoutes() {
    ///     return (
    ///       <>
    ///         <Route path="/" component={Home} />
    ///         <Route path="/landing" component={Landing} />
    ///         …
    ///       </>
    ///     );
    ///   }
    ///
    /// Returning an `ssr` template literal here (i.e. just `result.merge`-ing
    /// every child) would force each `<Route>` into `escape(createComponent(…))`,
    /// stringifying them at template-construction time. The `Router` then
    /// receives a string of HTML instead of an array of `Route` descriptors,
    /// fails to build the route table, and renders nothing — which surfaces
    /// as an empty `<main></main>` server-side.
    fn transform_fragment(&self, fragment: &JSXFragment<'a>) -> SSRResult<'a> {
        let ast = self.context.ast();
        let hydratable = self.context.hydratable && self.options.hydratable;
        let mut result = SSRResult::new();
        result.span = fragment.span;

        // Reduce each child to a single JS expression. Empty/whitespace-only
        // text nodes are dropped, matching `filterChildren` in Babel.
        let mut child_exprs: Vec<Expression<'a>> = Vec::new();
        for child in &fragment.children {
            let child_result = match self.transform_node(child) {
                Some(r) => r,
                None => continue,
            };
            // Empty result (no template parts AND no values) → skip.
            if child_result.template_values.is_empty()
                && child_result.template_parts.iter().all(|p| p.is_empty())
            {
                continue;
            }
            child_exprs.push(child_result.to_ssr_expression(&self.context, hydratable));
        }

        if child_exprs.is_empty() {
            // Empty fragment — leave the result empty (becomes `""` when
            // realised). Babel similarly produces an empty array / no content.
            return result;
        }

        let combined = if child_exprs.len() == 1 {
            child_exprs.pop().unwrap()
        } else {
            // Multi-child fragment → JS array literal so Solid's renderer (and
            // route-table walking) can iterate the children directly.
            let mut elements = ast.vec_with_capacity(child_exprs.len());
            for expr in child_exprs {
                elements.push(ArrayExpressionElement::from(expr));
            }
            ast.expression_array(SPAN, elements)
        };

        // Push as a single dynamic value with no surrounding template parts.
        // The short-circuit in `to_ssr_expression` will return `combined` bare
        // when this result is realised at the call site (e.g. the `Router`'s
        // `children` getter receives the array directly, not an `ssr` string).
        result.push_dynamic(combined, false, false);
        result
    }

    /// Transform JSX text
    fn transform_text(&self, text: &JSXText<'a>) -> Option<SSRResult<'a>> {
        let content = common::expression::trim_whitespace(&text.value);
        if content.is_empty() {
            return None;
        }

        let mut result = SSRResult::new();
        result.span = text.span;
        result.push_static(&common::expression::escape_html(&content, false));
        Some(result)
    }

    /// Transform a JSX expression container
    fn transform_expression_container(
        &self,
        container: &JSXExpressionContainer<'a>,
    ) -> Option<SSRResult<'a>> {
        if let Some(expr) = container.expression.as_expression() {
            self.context.register_helper("escape");
            let mut result = SSRResult::new();
            result.span = container.span;
            result.push_dynamic(self.context.clone_expr(expr), false, false);
            Some(result)
        } else {
            None
        }
    }
}

impl<'a> Traverse<'a, ()> for SSRTransform<'a> {
    // Use exit_expression instead of enter_expression to avoid
    // oxc_traverse walking into our newly created nodes (which lack scope info)
    fn exit_expression(&mut self, node: &mut Expression<'a>, ctx: &mut TraverseCtx<'a, ()>) {
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
        let span = SPAN;
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
            // Only pure named imports are safe to augment. Mixing named with namespace
            // (and in some toolchains default/namespace variants) can emit invalid JS.
            if is_target_module && first_augmentable_module_import_index.is_none() {
                if let Some(specifiers) = &import_decl.specifiers {
                    let all_named_specifiers = !specifiers.is_empty()
                        && specifiers.iter().all(|spec| {
                            matches!(spec, ImportDeclarationSpecifier::ImportSpecifier(_))
                        });
                    if all_named_specifiers {
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
            let imported = ModuleExportName::IdentifierName(ast.identifier_name(span, helper_str));
            let local = ast.binding_identifier(span, helper_str);
            let specifier = ast.import_specifier(span, imported, local, ImportOrExportKind::Value);
            specifiers.push(ImportDeclarationSpecifier::ImportSpecifier(
                ast.alloc(specifier),
            ));
        }

        if specifiers.is_empty() {
            return;
        }

        // Prefer augmenting the first existing import from the module to avoid extra imports.
        if let Some(import_index) = first_augmentable_module_import_index {
            if let Statement::ImportDeclaration(import_decl) = &mut program.body[import_index] {
                let decl_specifiers = import_decl.specifiers.get_or_insert_with(|| ast.vec());
                decl_specifiers.extend(specifiers);
                return;
            }
        }

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

        // Insert at the beginning of the program
        program.body.insert(0, import_stmt);
    }
}

impl<'a> SSRTransform<'a> {
    /// Build the SSR expression from the transform result
    fn build_ssr_expression(
        &self,
        result: &SSRResult<'a>,
        _ctx: &mut TraverseCtx<'a, ()>,
    ) -> Expression<'a> {
        let hydratable = self.context.hydratable && self.options.hydratable;

        // `to_ssr_expression` self-registers `ssr` and `escape` when it
        // actually emits them (see `ir.rs`). We don't need to pre-register
        // them here — the previous gate via `needs_ssr_wrapper()` was correct
        // for this site but missed the other three sites that call
        // `to_ssr_expression` directly.
        result.to_ssr_expression(&self.context, hydratable)
    }
}
