//! Intermediate Representation for SSR transforms
//!
//! SSR uses a simpler IR than DOM since we're just building template strings.

use indexmap::IndexSet;
use oxc_allocator::{Allocator, CloneIn};
use oxc_ast::ast::JSXChild;
use oxc_ast::ast::{Argument, Expression, TemplateElementValue};
use oxc_ast::AstBuilder;
use oxc_span::{Span, SPAN};
use std::cell::RefCell;

use common::expr_to_string;

/// Function type for transforming child JSX elements
pub type SSRChildTransformer<'a, 'b> = &'b dyn Fn(&JSXChild<'a>) -> Option<SSRResult<'a>>;

/// The result of transforming a JSX node for SSR
pub struct SSRResult<'a> {
    /// Source span of the originating JSX node
    pub span: Span,

    /// Static template parts (the strings between dynamic values)
    pub template_parts: Vec<String>,

    /// Dynamic values to be interpolated (wrapped in escape())
    pub template_values: Vec<TemplateValue<'a>>,

    /// Whether this needs a hydration key
    pub needs_hydration_key: bool,

    /// Whether to skip escaping (for innerHTML, script, style)
    pub skip_escape: bool,

    /// Whether this contains a spread attribute
    pub has_spread: bool,

    /// The tag name (for native elements)
    pub tag_name: Option<String>,
}

/// A dynamic value in the SSR template
pub struct TemplateValue<'a> {
    /// The expression AST
    pub expr: Expression<'a>,

    /// Whether this is an attribute value (uses different escaping)
    pub is_attr: bool,

    /// Whether to skip escaping entirely
    pub skip_escape: bool,

    /// Whether this needs hydration markers (for dynamic children)
    pub needs_hydration_marker: bool,
}

impl<'a> Default for SSRResult<'a> {
    fn default() -> Self {
        Self {
            span: SPAN,
            template_parts: Vec::new(),
            template_values: Vec::new(),
            needs_hydration_key: false,
            skip_escape: false,
            has_spread: false,
            tag_name: None,
        }
    }
}

impl<'a> SSRResult<'a> {
    /// Create a new empty SSR result
    pub fn new() -> Self {
        Self::default()
    }

    /// Append static text to the template
    pub fn push_static(&mut self, text: &str) {
        if self.template_parts.is_empty() {
            self.template_parts.push(text.to_string());
        } else {
            let last = self.template_parts.last_mut().unwrap();
            last.push_str(text);
        }
    }

    /// Append a dynamic value.
    ///
    /// Hydration markers (`<!--$-->`/`<!--/-->`) are NOT auto-emitted around
    /// the value. Babel's `babel-plugin-jsx-dom-expressions` only wraps a
    /// dynamic child with markers when its **parent element has more than one
    /// meaningful child** ("multi" rule). Auto-marking every dynamic value
    /// here would produce markers around solitary children which `getNextMarker`
    /// then mis-locates, causing hydration mismatches and full client re-render.
    /// Marker emission is therefore the parent's responsibility — see
    /// `process_jsx_children` in `element.rs` which inserts explicit
    /// `<!--$-->`/`<!--/-->` static parts when `multi && hydratable`.
    pub fn push_dynamic(&mut self, expr: Expression<'a>, is_attr: bool, skip_escape: bool) {
        self.push_dynamic_with_marker(expr, is_attr, skip_escape, false)
    }

    /// Append a dynamic value with explicit hydration marker control
    pub fn push_dynamic_with_marker(
        &mut self,
        expr: Expression<'a>,
        is_attr: bool,
        skip_escape: bool,
        needs_marker: bool,
    ) {
        // Ensure we have a template part before this value
        if self.template_parts.len() == self.template_values.len() {
            self.template_parts.push(String::new());
        }
        self.template_values.push(TemplateValue {
            expr,
            is_attr,
            skip_escape,
            needs_hydration_marker: needs_marker,
        });
        // Add empty part for after this value
        self.template_parts.push(String::new());
    }

    /// Merge another SSR result into this one
    pub fn merge(&mut self, other: SSRResult<'a>) {
        for (i, part) in other.template_parts.into_iter().enumerate() {
            if i == 0 && !self.template_parts.is_empty() {
                // Merge first part with our last part
                self.template_parts.last_mut().unwrap().push_str(&part);
            } else {
                self.template_parts.push(part);
            }
        }
        self.template_values.extend(other.template_values);
    }

    /// Generate the final ssr tagged template call
    pub fn to_ssr_call(&self) -> String {
        self.to_ssr_call_with_hydration(false)
    }

    /// Generate the final ssr tagged template call with optional hydration markers
    pub fn to_ssr_call_with_hydration(&self, hydratable: bool) -> String {
        if self.template_values.is_empty() {
            // No dynamic values, just return static string
            format!("\"{}\"", self.template_parts.join(""))
        } else {
            // Build ssr`...` tagged template
            let mut result = String::from("ssr`");

            for (i, part) in self.template_parts.iter().enumerate() {
                result.push_str(part);
                if i < self.template_values.len() {
                    let val = &self.template_values[i];

                    // Add hydration marker before dynamic content (not for attributes)
                    if hydratable && !val.is_attr && val.needs_hydration_marker {
                        result.push_str("<!--$-->");
                    }

                    result.push_str("${");
                    if val.skip_escape {
                        result.push_str(&expr_to_string(&val.expr));
                    } else if val.is_attr {
                        result.push_str(&format!("escape({}, true)", expr_to_string(&val.expr)));
                    } else {
                        result.push_str(&format!("escape({})", expr_to_string(&val.expr)));
                    }
                    result.push('}');

                    // Add closing hydration marker
                    if hydratable && !val.is_attr && val.needs_hydration_marker {
                        result.push_str("<!--/-->");
                    }
                }
            }

            result.push('`');
            result
        }
    }

    /// Returns true when `to_ssr_expression` will actually emit an `ssr`...``
    /// tagged-template call (and therefore the `ssr` runtime helper must be
    /// imported). Returns false for the trivial cases that short-circuit:
    /// pure-static (just a string literal) and a single dynamic expression
    /// with no static template content (returns the expression bare, matching
    /// Babel's `if (!result.template) return result.exprs[0]`).
    pub fn needs_ssr_wrapper(&self) -> bool {
        if self.template_values.is_empty() {
            return false;
        }
        if self.template_values.len() == 1
            && self.template_parts.iter().all(|p| p.is_empty())
            && !self.template_values[0].is_attr
            && !self.template_values[0].needs_hydration_marker
        {
            return false;
        }
        true
    }

    /// Lower the IR to its final JS expression. Registers the runtime helpers
    /// (`ssr`, `escape`) it emits on `context` itself so every call site is
    /// guaranteed-correct without having to re-derive what the IR will produce.
    /// Without that self-registration, paths that bypass
    /// `SSRTransform::build_ssr_expression` (native-element children in
    /// `element.rs`, component children in `component.rs`, fragment children
    /// in `transform.rs`) emit bare `ssr\`…\`` tagged templates whose `ssr`
    /// import never gets added to the file — which surfaces at SSR runtime as
    /// `ReferenceError: ssr is not defined` once the bundler renames or
    /// deconflicts the import binding.
    pub fn to_ssr_expression(
        &self,
        context: &SSRContext<'a>,
        hydratable: bool,
    ) -> Expression<'a> {
        let ast = context.ast();
        let gen_span = SPAN;

        if self.template_values.is_empty() {
            let content = self.template_parts.join("");
            let allocated_str = ast.allocator.alloc_str(&content);
            return ast.expression_string_literal(gen_span, allocated_str, None);
        }

        // Babel-parity short-circuit (see `babel-plugin-jsx-dom-expressions`'s
        // `createTemplate` in `src/ssr/template.js`):
        //
        //     if (!result.template) {
        //       return result.exprs[0];
        //     }
        //
        // When a JSX node has produced no static template content — only a
        // single dynamic expression — return that expression directly. This
        // matters for two distinct reasons:
        //
        // 1. Spread/`ssrElement(...)` shape: wrapping in `ssr`${expr}`` would
        //    produce identical HTML, but parents that detect
        //    `is_call_expression(ssrElement, ...)` to short-circuit instead see
        //    a TaggedTemplate and take a different code path. In a hydratable
        //    build that drift produces a different `getNextContextId()`
        //    allocation order on the SSR vs DOM side and surfaces as
        //      `Hydration Mismatch. Unable to find DOM nodes for hydration key`
        //    even though the rendered HTML structure is identical.
        //
        // 2. Component laziness: a top-level `<MyComponent />` (or a single
        //    component inside a fragment, or a JSX expression) becomes a bare
        //    `createComponent(MyComponent, {})` call — NOT
        //    `ssr`${escape(createComponent(MyComponent, {}))}``. The bare form
        //    is what consumers expect when they pass JSX into a prop
        //    (e.g. `<Show fallback={<MyComponent />}>`): they want a
        //    component reference, not a synchronously-rendered HTML string.
        //    Wrapping forces the component to render eagerly and breaks
        //    Suspense semantics for any async resources inside.
        //
        // Babel does NOT check skip_escape here either — `if (!result.template)`
        // is the entire check. Callers that need escape apply it themselves
        // (via `transformChildren` for native-element children).
        if self.template_values.len() == 1
            && self.template_parts.iter().all(|p| p.is_empty())
            && !self.template_values[0].is_attr
            && !self.template_values[0].needs_hydration_marker
        {
            return self.template_values[0].expr.clone_in(ast.allocator);
        }

        // Build quasis (static template parts)
        let mut quasis = ast.vec();
        let mut closing_marker_prefix = String::new();
        for (i, part) in self.template_parts.iter().enumerate() {
            let mut raw = String::new();
            raw.push_str(&closing_marker_prefix);
            closing_marker_prefix.clear();
            raw.push_str(part);

            if i < self.template_values.len() {
                let val = &self.template_values[i];
                if hydratable && !val.is_attr && val.needs_hydration_marker {
                    raw.push_str("<!--$-->");
                    closing_marker_prefix.push_str("<!--/-->");
                }
            }

            let is_tail = i == self.template_parts.len() - 1;
            let part_str = ast.allocator.alloc_str(&raw);
            let value = TemplateElementValue {
                raw: ast.atom(part_str),
                cooked: Some(ast.atom(part_str)),
            };
            let element = ast.template_element(gen_span, value, is_tail, true);
            quasis.push(element);
        }

        // We're definitely emitting an `ssr\`…\`` tagged template now — the
        // two short-circuits above have returned. Register the helper here so
        // every entry into this function gets a consistent import, regardless
        // of whether the caller went through `build_ssr_expression` or called
        // us directly (native-element children, component children, fragment
        // children).
        context.register_helper("ssr");

        // Build expressions (dynamic parts)
        let mut expressions = ast.vec();
        for val in &self.template_values {
            let expr = val.expr.clone_in(ast.allocator);
            let wrapped = if val.skip_escape {
                expr
            } else {
                // Same reasoning as `ssr` above: register `escape` whenever we
                // actually emit an `escape(...)` call. Most call paths already
                // pre-register it via `transform_expression_container`, but
                // the IR is the source of truth for what's emitted.
                context.register_helper("escape");
                let callee = ast.expression_identifier(gen_span, "escape");
                let mut args = ast.vec();
                args.push(Argument::from(expr));
                if val.is_attr {
                    let true_lit = ast.expression_boolean_literal(gen_span, true);
                    args.push(Argument::from(true_lit));
                }
                ast.expression_call(
                    gen_span,
                    callee,
                    None::<oxc_ast::ast::TSTypeParameterInstantiation<'a>>,
                    args,
                    false,
                )
            };
            expressions.push(wrapped);
        }

        // Build the template literal
        let template = ast.template_literal(gen_span, quasis, expressions);

        // Build the tag (ssr identifier)
        let tag = ast.expression_identifier(gen_span, "ssr");

        ast.expression_tagged_template(
            gen_span,
            tag,
            None::<oxc_ast::ast::TSTypeParameterInstantiation<'a>>,
            template,
        )
    }
}

/// Context for SSR block transformation
pub struct SSRContext<'a> {
    /// Helper imports needed
    pub helpers: RefCell<IndexSet<String>>,

    /// Variable counter for unique names
    pub var_counter: RefCell<usize>,

    /// Whether we're in hydratable mode
    pub hydratable: bool,

    allocator: &'a Allocator,
}

impl<'a> SSRContext<'a> {
    pub fn new(allocator: &'a Allocator, hydratable: bool) -> Self {
        Self {
            helpers: RefCell::new(IndexSet::new()),
            var_counter: RefCell::new(0),
            hydratable,
            allocator,
        }
    }

    /// Generate a unique variable name
    pub fn generate_uid(&self, prefix: &str) -> String {
        let mut counter = self.var_counter.borrow_mut();
        *counter += 1;
        format!("_{}{}", prefix, *counter)
    }

    /// Register a helper import
    pub fn register_helper(&self, name: &str) {
        self.helpers.borrow_mut().insert(name.to_string());
    }

    pub fn ast(&self) -> AstBuilder<'a> {
        AstBuilder::new(self.allocator)
    }

    pub fn clone_expr(&self, expr: &Expression<'a>) -> Expression<'a> {
        expr.clone_in(self.allocator)
    }
}
