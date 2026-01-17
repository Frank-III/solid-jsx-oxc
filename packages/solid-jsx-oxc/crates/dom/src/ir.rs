//! Intermediate Representation for Solid JSX transforms
//! This IR is used to collect information during traversal
//! and then generate code in a second pass.

use indexmap::IndexSet;
use oxc_allocator::{Allocator, CloneIn};
use oxc_ast::ast::{Expression, JSXChild};
use oxc_ast::AstBuilder;
use oxc_span::Span;
use std::cell::RefCell;

/// Function type for transforming child JSX elements
pub type ChildTransformer<'a, 'b> = &'b dyn Fn(&JSXChild<'a>) -> Option<TransformResult<'a>>;

/// The result of transforming a JSX node
#[derive(Default)]
pub struct TransformResult<'a> {
    /// Source span of the originating JSX node
    pub span: Span,

    /// The HTML template string
    pub template: String,

    /// Template with all closing tags (for SSR)
    pub template_with_closing_tags: String,

    /// Variable declarations needed
    pub declarations: Vec<Declaration<'a>>,

    /// Expressions to execute (effects, inserts, etc.)
    pub exprs: Vec<Expression<'a>>,

    /// Dynamic attribute bindings
    pub dynamics: Vec<DynamicBinding<'a>>,

    /// Post-expressions (run after main effects)
    pub post_exprs: Vec<Expression<'a>>,

    /// Whether this is SVG
    pub is_svg: bool,

    /// Whether this contains custom elements
    pub has_custom_element: bool,

    /// The tag name (for native elements)
    pub tag_name: Option<String>,

    /// Whether to skip template generation
    pub skip_template: bool,

    /// The generated element ID
    pub id: Option<String>,

    /// Whether this result is just text
    pub text: bool,

    /// Whether this result needs memo() wrapping (for fragment expressions)
    pub needs_memo: bool,

    /// Individual child codes for fragments (when children need to be in an array)
    pub child_results: Vec<TransformResult<'a>>,
}

/// A variable declaration
pub struct Declaration<'a> {
    pub name: String,
    pub init: Expression<'a>,
}

/// A dynamic attribute binding that needs effect wrapping
pub struct DynamicBinding<'a> {
    pub elem: String,
    pub key: String,
    pub value: Expression<'a>,
    pub is_svg: bool,
    pub is_ce: bool,
    pub tag_name: String,
}

/// Context for the current block being transformed
pub struct BlockContext<'a> {
    /// Current template string being built
    pub template: RefCell<String>,

    /// Templates collected at the file level
    pub templates: RefCell<Vec<TemplateInfo>>,

    /// Helper imports needed
    pub helpers: RefCell<IndexSet<String>>,

    /// Delegated events
    pub delegates: RefCell<IndexSet<String>>,

    /// Variable counter for unique names
    pub var_counter: RefCell<usize>,

    allocator: &'a Allocator,
}

pub struct TemplateInfo {
    pub content: String,
    pub is_svg: bool,
    pub span: Span,
}

impl<'a> BlockContext<'a> {
    pub fn new(allocator: &'a Allocator) -> Self {
        Self {
            template: RefCell::new(String::new()),
            templates: RefCell::new(Vec::new()),
            helpers: RefCell::new(IndexSet::new()),
            delegates: RefCell::new(IndexSet::new()),
            var_counter: RefCell::new(0),
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

    /// Register a delegated event
    pub fn register_delegate(&self, event: &str) {
        self.delegates.borrow_mut().insert(event.to_string());
    }

    /// Push a template and return its index
    pub fn push_template(&self, content: String, is_svg: bool, span: Span) -> usize {
        self.register_helper("template");
        let mut templates = self.templates.borrow_mut();
        let index = templates.len();
        templates.push(TemplateInfo {
            content,
            is_svg,
            span,
        });
        index
    }

    pub fn ast(&self) -> AstBuilder<'a> {
        AstBuilder::new(self.allocator)
    }

    pub fn clone_expr(&self, expr: &Expression<'a>) -> Expression<'a> {
        expr.clone_in(self.allocator)
    }
}
