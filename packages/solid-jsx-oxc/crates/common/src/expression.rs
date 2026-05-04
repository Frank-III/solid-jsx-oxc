//! Expression utilities for working with OXC AST

use oxc_ast::ast::{Expression, JSXChild, JSXElement, Statement};
use oxc_codegen::{Codegen, CodegenOptions};
use oxc_span::Span;

use crate::xhtml::lookup_xhtml_entity;

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

/// Decode JSX HTML entities (`&copy;`, `&#1234;`, `&#xAB;`) in a string.
///
/// OXC's JSX parser, unlike Babel's, leaves the source bytes of `JSXText` and
/// JSX string-attribute values verbatim — so a `&copy;` written in `.tsx`
/// arrives here as the eight literal characters `&`, `c`, `o`, `p`, `y`, `;`.
/// Without decoding, the SSR runtime's `escape` helper escapes the leading
/// `&` to `&amp;` and the browser then renders the literal text `&copy;`.
///
/// We mirror babel-parser's `jsxReadEntity` (in
/// `babel-parser/src/plugins/jsx/index.ts`) precisely:
///
/// * `&#NNNN;` and `&#xHHHH;` decode via `char::from_u32` if a valid code
///   point and the closing `;` is present within the digits.
/// * `&name;` decodes via the XHTML 1.0 named-entity table in `xhtml.rs` if
///   the name plus `;` is found within ten characters of the `&`.
/// * Anything else — no `;` within ten chars, name not in the table,
///   numeric overflow, missing `;` after digits — falls back to a literal
///   `&` and parsing resumes from the *next* character. So `&unknown;`
///   becomes `&unknown;` (literal); `&` followed by EOF stays as `&`.
///
/// The ten-character cap on entity-name length matches babel-parser's
/// behavior; it keeps a stray `&` in prose from scanning forward
/// indefinitely searching for a `;`.
pub fn decode_jsx_entities(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(input.len());
    let mut i = 0usize;

    while i < bytes.len() {
        // Fast path: anything that isn't `&` is copied as-is. We're working
        // in bytes for the search but copying the actual char (which might
        // be multi-byte UTF-8) below.
        if bytes[i] != b'&' {
            // Find the next `&` (or end of string), copy that whole slice.
            let start = i;
            while i < bytes.len() && bytes[i] != b'&' {
                i += 1;
            }
            out.push_str(&input[start..i]);
            continue;
        }

        // We're at `&`. Try to parse an entity. If it fails, emit `&` literal
        // and advance by 1 (matching babel-parser's "Not a valid entity" path
        // that resets pos to `startPos = ++this.state.pos`).
        let entity_start = i;
        i += 1; // consume `&`

        if i >= bytes.len() {
            out.push('&');
            break;
        }

        // Numeric entity: `&#…;`
        if bytes[i] == b'#' {
            let after_hash = i + 1;
            let (radix, digits_start) = if after_hash < bytes.len()
                && (bytes[after_hash] == b'x' || bytes[after_hash] == b'X')
            {
                (16u32, after_hash + 1)
            } else {
                (10u32, after_hash)
            };

            let mut digit_end = digits_start;
            while digit_end < bytes.len() && is_digit_in(bytes[digit_end], radix) {
                digit_end += 1;
            }

            if digit_end > digits_start
                && digit_end < bytes.len()
                && bytes[digit_end] == b';'
            {
                if let Ok(num) = u32::from_str_radix(&input[digits_start..digit_end], radix as u32)
                {
                    if let Some(ch) = char::from_u32(num) {
                        out.push(ch);
                        i = digit_end + 1;
                        continue;
                    }
                }
            }

            // Invalid numeric — fall through to "literal &" path below.
            out.push('&');
            i = entity_start + 1;
            continue;
        }

        // Named entity: `&name;` where name is up to 10 chars.
        let mut scan = i;
        let max_scan = (i + 10).min(bytes.len());
        let mut found_semi = false;
        while scan < max_scan {
            if bytes[scan] == b';' {
                found_semi = true;
                break;
            }
            scan += 1;
        }

        if found_semi {
            let name = &input[i..scan];
            if let Some(decoded) = lookup_xhtml_entity(name) {
                out.push_str(decoded);
                i = scan + 1;
                continue;
            }
        }

        // Not a recognized entity. Emit literal `&`, advance one char past
        // the `&` (babel-parser semantics: pos = startPos which was
        // ++this.state.pos, so we resume one byte after `&`).
        out.push('&');
        i = entity_start + 1;
    }

    out
}

fn is_digit_in(b: u8, radix: u32) -> bool {
    if radix == 10 {
        b.is_ascii_digit()
    } else {
        b.is_ascii_hexdigit()
    }
}

/// Trim whitespace from JSX text following the JSX-text whitespace rules.
///
/// This is a direct port of `trimWhitespace` in
/// `babel-plugin-jsx-dom-expressions/src/shared/utils.js`:
///
/// ```js
/// function trimWhitespace(text) {
///   text = text.replace(/\r/g, "");
///   if (/\n/g.test(text)) {
///     text = text
///       .split("\n")
///       .map((t, i) => (i ? t.replace(/^\s*/g, "") : t))
///       .filter(s => !/^\s*$/.test(s))
///       .join(" ");
///   }
///   return text.replace(/\s+/g, " ");
/// }
/// ```
///
/// The previous Rust implementation called `result.trim()` for any text that
/// contained a newline, which incorrectly dropped *trailing* significant
/// whitespace — e.g. `"\n      Welcome back, "` (the JSX text between
/// `<p class="lead">` and `<strong>`) became `"Welcome back,"` instead of
/// `"Welcome back, "`. That swallowed the space the author intended between
/// the prose and the inline element. Babel keeps the trailing space because
/// only *whitespace-only* split-lines are filtered out; the line containing
/// real content keeps its trailing whitespace, which then survives the final
/// `\s+ → " "` collapse.
pub fn trim_whitespace(text: &str) -> String {
    // NOTE: we do NOT decode JSX HTML entities here. Babel's transform reads
    // JSX text via `node.extra.raw` (the verbatim source bytes) for native
    // elements and lets the entity form survive into the final HTML
    // template — the browser decodes `&copy;` at parse time. OXC's
    // `JSXText.value` is already the verbatim source (it doesn't decode
    // entities at parse time either), so passing it through unchanged here
    // matches Babel's output byte-for-byte. `decode_jsx_entities` is still
    // exported for the few sites — like Babel's component-children path —
    // that explicitly decode before re-injecting into a JS string literal.
    //
    // Strip CR (mirror of `text.replace(/\r/g, "")`).
    let stripped: String = text.chars().filter(|c| *c != '\r').collect();

    let mut joined = if stripped.contains('\n') {
        stripped
            .split('\n')
            .enumerate()
            .map(|(i, line)| {
                if i == 0 {
                    line.to_string()
                } else {
                    // Equivalent to JS `.replace(/^\s*/g, "")` — strip leading
                    // whitespace from every non-first line (the indentation).
                    line.trim_start().to_string()
                }
            })
            .filter(|line| !line.chars().all(|c| c.is_whitespace()))
            .collect::<Vec<_>>()
            .join(" ")
    } else {
        stripped
    };

    // Final pass: collapse `\s+` to a single space (mirror of
    // `text.replace(/\s+/g, " ")`).
    let mut collapsed = String::with_capacity(joined.len());
    let mut prev_ws = false;
    for c in joined.drain(..) {
        if c.is_whitespace() {
            if !prev_ws {
                collapsed.push(' ');
                prev_ws = true;
            }
        } else {
            collapsed.push(c);
            prev_ws = false;
        }
    }
    collapsed
}

/// Convert event name from JSX format (onClick or on:click) to DOM format (click)
pub fn to_event_name(name: &str) -> String {
    if name.starts_with("on:") {
        // Handle on:click -> click (namespaced form)
        name[3..].to_string()
    } else if name.starts_with("on") {
        // Handle onClick -> click, onMouseDown -> mousedown (lowercase entire name)
        name[2..].to_lowercase()
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
