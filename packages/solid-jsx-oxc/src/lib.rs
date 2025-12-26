//! Solid JSX OXC Compiler
//!
//! A Rust-based JSX compiler for SolidJS using OXC.
//! This is a port of babel-plugin-jsx-dom-expressions to OXC.
//!
//! ## Usage
//!
//! ```rust
//! use solid_jsx_oxc::{transform, TransformOptions};
//!
//! let source = r#"<div class="hello">{count()}</div>"#;
//! let result = transform(source, None);
//! println!("{}", result.code);
//! ```

pub use common::TransformOptions;

#[cfg(feature = "napi")]
use napi_derive::napi;

use oxc_allocator::Allocator;
use oxc_codegen::{Codegen, CodegenReturn, CodegenOptions, IndentChar};
use oxc_parser::Parser;
use oxc_span::SourceType;

use std::path::PathBuf;

use dom::SolidTransform;
use ssr::SSRTransform;

/// Result of a transform operation
#[cfg(feature = "napi")]
#[napi(object)]
pub struct TransformResult {
    /// The transformed code
    pub code: String,
    /// Source map (if enabled)
    pub map: Option<String>,
}

/// Transform options exposed to JavaScript
#[cfg(feature = "napi")]
#[napi(object)]
#[derive(Default)]
pub struct JsTransformOptions {
    /// The module to import runtime helpers from
    /// @default "solid-js/web"
    pub module_name: Option<String>,

    /// Generate mode: "dom", "ssr", or "universal"
    /// @default "dom"
    pub generate: Option<String>,

    /// Whether to enable hydration support
    /// @default false
    pub hydratable: Option<bool>,

    /// Whether to delegate events
    /// @default true
    pub delegate_events: Option<bool>,

    /// Whether to wrap conditionals
    /// @default true
    pub wrap_conditionals: Option<bool>,

    /// Whether to pass context to custom elements
    /// @default true
    pub context_to_custom_elements: Option<bool>,

    /// Source filename
    /// @default "input.jsx"
    pub filename: Option<String>,

    /// Whether to generate source maps
    /// @default false
    pub source_map: Option<bool>,
}

/// Transform JSX source code
#[cfg(feature = "napi")]
#[napi]
pub fn transform_jsx(source: String, options: Option<JsTransformOptions>) -> TransformResult {
    let js_options = options.unwrap_or_default();

    // Convert JS options to internal options
    let generate = match js_options.generate.as_deref() {
        Some("ssr") => common::GenerateMode::Ssr,
        Some("universal") => common::GenerateMode::Universal,
        _ => common::GenerateMode::Dom,
    };

    let options = TransformOptions {
        generate,
        hydratable: js_options.hydratable.unwrap_or(false),
        delegate_events: js_options.delegate_events.unwrap_or(true),
        wrap_conditionals: js_options.wrap_conditionals.unwrap_or(true),
        context_to_custom_elements: js_options.context_to_custom_elements.unwrap_or(true),
        filename: js_options.filename.as_deref().unwrap_or("input.jsx"),
        source_map: js_options.source_map.unwrap_or(false),
        ..TransformOptions::solid_defaults()
    };

    let result = transform_internal(&source, &options);

    TransformResult {
        code: result.code,
        map: result.map.map(|m| m.to_json_string()),
    }
}

/// Internal transform function
pub fn transform(source: &str, options: Option<TransformOptions>) -> CodegenReturn {
    let options = options.unwrap_or_else(TransformOptions::solid_defaults);
    transform_internal(source, &options)
}

fn transform_internal(source: &str, options: &TransformOptions) -> CodegenReturn {
    let allocator = Allocator::default();
    let source_type = SourceType::from_path(options.filename).unwrap_or(SourceType::tsx());

    // Parse the source
    let mut program = Parser::new(&allocator, source, source_type)
        .parse()
        .program;

    // Run the appropriate transform based on generate mode
    // SAFETY: We create a raw pointer to `options` and dereference it to get a reference
    // with an independent lifetime. This is safe because:
    // 1. `options` is borrowed for the entire duration of this function
    // 2. The reference is only used within this function's scope
    // 3. The transformers don't outlive this function
    // This pattern is used to work around Rust's borrow checker limitations with
    // multiple mutable borrows needed during AST traversal.
    let options_ref = unsafe { &*(options as *const TransformOptions) };

    match options.generate {
        common::GenerateMode::Dom => {
            let transformer = SolidTransform::new(&allocator, options_ref);
            transformer.transform(&mut program);
        }
        common::GenerateMode::Ssr => {
            let transformer = SSRTransform::new(&allocator, options_ref);
            transformer.transform(&mut program);
        }
        common::GenerateMode::Universal => {
            // Universal mode generates DOM with SSR fallback markers
            // For now, use DOM transform
            let transformer = SolidTransform::new(&allocator, options_ref);
            transformer.transform(&mut program);
        }
    }

    // Generate code
    Codegen::new()
        .with_options(CodegenOptions {
            source_map_path: if options.source_map {
                Some(PathBuf::from(options.filename))
            } else {
                None
            },
            indent_width: 2,
            indent_char: IndentChar::Space,
            ..CodegenOptions::default()
        })
        .build(&program)
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_element() {
        let source = r#"<div class="hello">world</div>"#;
        let result = transform(source, None);
        // The transform should produce valid code
        assert!(!result.code.is_empty());
    }

    #[test]
    fn test_dynamic_attribute() {
        let source = r#"<div class={style()}>content</div>"#;
        let result = transform(source, None);
        assert!(!result.code.is_empty());
    }

    #[test]
    fn test_component() {
        let source = r#"<Button onClick={handler}>Click me</Button>"#;
        let result = transform(source, None);
        assert!(!result.code.is_empty());
    }

    #[test]
    fn test_for_loop() {
        let source = r#"<For each={items}>{item => <div>{item}</div>}</For>"#;
        let result = transform(source, None);
        assert!(!result.code.is_empty());
    }

    #[test]
    fn test_ssr_basic_element() {
        let source = r#"<div class="hello">world</div>"#;
        let options = TransformOptions {
            generate: common::GenerateMode::Ssr,
            ..TransformOptions::solid_defaults()
        };
        let result = transform(source, Some(options));
        assert!(!result.code.is_empty());
    }

    #[test]
    fn test_ssr_dynamic_attribute() {
        let source = r#"<div class={style()}>content</div>"#;
        let options = TransformOptions {
            generate: common::GenerateMode::Ssr,
            ..TransformOptions::solid_defaults()
        };
        let result = transform(source, Some(options));
        assert!(!result.code.is_empty());
    }

    #[test]
    fn test_ssr_component() {
        let source = r#"<Button onClick={handler}>Click me</Button>"#;
        let options = TransformOptions {
            generate: common::GenerateMode::Ssr,
            ..TransformOptions::solid_defaults()
        };
        let result = transform(source, Some(options));
        assert!(!result.code.is_empty());
    }

    #[test]
    fn test_ssr_output_preview() {
        // Test various SSR outputs
        let cases = [
            (r#"<div class="hello">world</div>"#, "basic element"),
            (r#"<div class={style()}>content</div>"#, "dynamic class"),
            (r#"<div>{count()}</div>"#, "dynamic child"),
            (r#"<For each={items}>{item => <li>{item}</li>}</For>"#, "For loop"),
            (r#"<Show when={visible}><div>shown</div></Show>"#, "Show conditional"),
            (r#"<Button><span>icon</span> Click</Button>"#, "component with JSX child"),
            (r#"<Show when={visible}><div class="content">shown</div></Show>"#, "Show with JSX child"),
        ];

        for (source, label) in cases {
            let options = TransformOptions {
                generate: common::GenerateMode::Ssr,
                ..TransformOptions::solid_defaults()
            };
            let result = transform(source, Some(options));
            println!("\n=== {} ===\nInput:  {}\nOutput: {}", label, source, result.code);
        }
    }

    #[test]
    fn test_dom_output_preview() {
        // Test various DOM outputs
        let cases = [
            (r#"<div class="hello">world</div>"#, "basic element"),
            (r#"<div class={style()}>content</div>"#, "dynamic class"),
            (r#"<div>{count()}</div>"#, "dynamic child"),
            (r#"<div onClick={handler}>click</div>"#, "event handler"),
            (r#"<Button onClick={handler}>Click me</Button>"#, "component"),
            (r#"<Button><span>icon</span> Click</Button>"#, "component with JSX child"),
            (r#"<Show when={visible}><div class="content">shown</div></Show>"#, "Show with JSX child"),
            (r#"<div><span class={style()}>nested dynamic</span></div>"#, "nested dynamic element"),
            (r#"<div><span onClick={handler}>nested event</span></div>"#, "nested event handler"),
            (r#"<div style={{ color: 'red', fontSize: 14 }}>styled</div>"#, "style object"),
            (r#"<div style={dynamicStyle()}>dynamic style</div>"#, "dynamic style"),
            (r#"<div innerHTML={html} />"#, "innerHTML"),
        ];

        for (source, label) in cases {
            // DOM mode is the default
            let result = transform(source, None);
            println!("\n=== DOM: {} ===\nInput:  {}\nOutput: {}", label, source, result.code);
        }
    }
}
