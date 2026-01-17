//! SSR template generation
//!
//! Unlike DOM which uses template() + cloneNode(), SSR uses
//! the ssr`` tagged template literal.

use crate::ir::{SSRContext, SSRResult};

/// Generate the final SSR output code from a result
pub fn generate_ssr_code(result: &SSRResult, context: &SSRContext<'_>) -> String {
    let mut code = String::new();

    // Generate helper imports
    let helpers = context.helpers.borrow();
    if !helpers.is_empty() {
        let helper_list: Vec<&String> = helpers.iter().collect();
        code.push_str(&format!(
            "import {{ {} }} from \"solid-js/web\";\n\n",
            helper_list
                .iter()
                .map(|h| h.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }

    // Generate the ssr call
    code.push_str(&result.to_ssr_call());

    code
}

/// Wrap a value in escape() call if needed
pub fn escape_value(expr: &str, is_attr: bool) -> String {
    if is_attr {
        format!("escape({}, true)", expr)
    } else {
        format!("escape({})", expr)
    }
}

/// Generate ssrAttribute call for dynamic boolean attributes
pub fn ssr_attribute(name: &str, expr: &str, is_boolean: bool) -> String {
    format!(
        "ssrAttribute(\"{}\", {}, {})",
        name,
        expr,
        if is_boolean { "true" } else { "false" }
    )
}

/// Generate ssrStyle call
pub fn ssr_style(expr: &str) -> String {
    format!("ssrStyle({})", expr)
}

/// Generate ssrClassList call
pub fn ssr_class_list(expr: &str) -> String {
    format!("ssrClassList({})", expr)
}

/// Generate ssrHydrationKey call
pub fn ssr_hydration_key() -> &'static str {
    "ssrHydrationKey()"
}
