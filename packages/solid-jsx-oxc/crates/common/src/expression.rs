//! Expression utilities for working with OXC AST

use oxc_ast::ast::{Expression, JSXChild, JSXElement, Statement};
use oxc_codegen::{Codegen, CodegenOptions};
use oxc_span::Span;

/// Convert an Expression AST node to its source code string
pub fn expr_to_string(expr: &Expression<'_>) -> String {
    let mut codegen = Codegen::new().with_options(CodegenOptions::default());
    codegen.print_expression(expr);
    codegen.into_source_text()
}

/// Convert a Statement AST node to its source code string
pub fn stmt_to_string(stmt: &Statement<'_>) -> String {
    // For statements, we need to wrap in a minimal program context
    // But for most cases we just need expression statements
    match stmt {
        Statement::ExpressionStatement(expr_stmt) => expr_to_string(&expr_stmt.expression),
        _ => {
            // Fallback - this is less common
            format!("/* unsupported statement */")
        }
    }
}

/// A simple expression node that tracks static vs dynamic
pub struct SimpleExpression<'a> {
    pub content: String,
    pub is_static: bool,
    pub expr: Option<&'a Expression<'a>>,
    pub span: Span,
}

impl<'a> SimpleExpression<'a> {
    pub fn static_value(content: String, span: Span) -> Self {
        Self {
            content,
            is_static: true,
            expr: None,
            span,
        }
    }

    pub fn dynamic(content: String, expr: &'a Expression<'a>, span: Span) -> Self {
        Self {
            content,
            is_static: false,
            expr: Some(expr),
            span,
        }
    }
}

/// Escape HTML special characters
pub fn escape_html(text: &str, quote_escape: bool) -> String {
    let mut result = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => result.push_str("&amp;"),
            '<' => result.push_str("&lt;"),
            '>' => result.push_str("&gt;"),
            '"' if quote_escape => result.push_str("&quot;"),
            '\'' if quote_escape => result.push_str("&#39;"),
            _ => result.push(c),
        }
    }
    result
}

/// Trim whitespace from JSX text (preserving significant spaces)
///
/// JSX whitespace rules:
/// - Text with newlines: trim leading/trailing whitespace (indentation)
/// - Inline text (no newlines): preserve trailing space (e.g., ". " between expressions)
/// - Multiple whitespace collapses to single space
pub fn trim_whitespace(text: &str) -> String {
    let has_newline = text.contains('\n');

    // Collapse multiple whitespace into single space
    let mut result = String::new();
    let mut prev_was_space = false;

    for c in text.chars() {
        if c.is_whitespace() {
            if has_newline {
                // Ignore leading indentation/newlines; we'll trim later.
                if !prev_was_space && !result.is_empty() {
                    result.push(' ');
                    prev_was_space = true;
                }
                continue;
            }

            // Inline text: preserve a single leading space (e.g., " Click" after an element)
            if !prev_was_space {
                result.push(' ');
                prev_was_space = true;
            }
        } else {
            result.push(c);
            prev_was_space = false;
        }
    }

    // Only trim if text contained newlines (multi-line JSX text with indentation)
    // Preserve trailing space for inline text like ". " between expressions
    if has_newline {
        result.trim().to_string()
    } else {
        result
    }
}

/// Convert event name from JSX format (onClick or on:click) to DOM format (click)
pub fn to_event_name(name: &str) -> String {
    if name.starts_with("on:") {
        // Handle on:click -> click (namespaced form)
        name[3..].to_string()
    } else if name.starts_with("on") {
        let event = &name[2..];
        // Handle onClick -> click (lowercase first char)
        if let Some(first) = event.chars().next() {
            format!("{}{}", first.to_lowercase(), &event[first.len_utf8()..])
        } else {
            String::new()
        }
    } else {
        name.to_string()
    }
}

/// Convert property name to proper case
pub fn to_property_name(name: &str) -> String {
    // Already camelCase, just return
    name.to_string()
}

/// Get children as a callback expression from a JSX element.
///
/// Used for control flow components (For, Index, etc.) that expect
/// arrow function children like: `<For each={items}>{item => <div>{item}</div>}</For>`
///
/// Returns the expression string, or "() => undefined" if no expression child found.
pub fn get_children_callback(element: &JSXElement<'_>) -> String {
    for child in &element.children {
        if let JSXChild::ExpressionContainer(container) = child {
            if let Some(expr) = container.expression.as_expression() {
                return expr_to_string(expr);
            }
        }
    }
    "() => undefined".to_string()
}
