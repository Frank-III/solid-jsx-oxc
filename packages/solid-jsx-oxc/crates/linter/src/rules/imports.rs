//! solid/imports
//!
//! Enforce consistent imports from "solid-js", "solid-js/web", and "solid-js/store".

use oxc_ast::ast::ImportDeclaration;

use crate::diagnostic::Diagnostic;
use crate::{RuleCategory, RuleMeta};

/// imports rule
#[derive(Debug, Clone, Default)]
pub struct Imports;

impl RuleMeta for Imports {
    const NAME: &'static str = "imports";
    const CATEGORY: RuleCategory = RuleCategory::Correctness;
}

/// Valid sources for Solid imports
const SOLID_SOURCES: &[&str] = &["solid-js", "solid-js/web", "solid-js/store"];

/// Primitives that should be imported from "solid-js"
const SOLID_JS_PRIMITIVES: &[&str] = &[
    "createSignal",
    "createEffect",
    "createMemo",
    "createResource",
    "onMount",
    "onCleanup",
    "onError",
    "untrack",
    "batch",
    "on",
    "createRoot",
    "getOwner",
    "runWithOwner",
    "mergeProps",
    "splitProps",
    "useTransition",
    "observable",
    "from",
    "mapArray",
    "indexArray",
    "createContext",
    "useContext",
    "children",
    "lazy",
    "createUniqueId",
    "createDeferred",
    "createRenderEffect",
    "createComputed",
    "createReaction",
    "createSelector",
    "DEV",
    "For",
    "Show",
    "Switch",
    "Match",
    "Index",
    "ErrorBoundary",
    "Suspense",
    "SuspenseList",
];

/// Primitives that should be imported from "solid-js/web"
const SOLID_WEB_PRIMITIVES: &[&str] = &[
    "Portal",
    "render",
    "hydrate",
    "renderToString",
    "renderToStream",
    "isServer",
    "renderToStringAsync",
    "generateHydrationScript",
    "HydrationScript",
    "Dynamic",
];

/// Primitives that should be imported from "solid-js/store"
const SOLID_STORE_PRIMITIVES: &[&str] = &[
    "createStore",
    "produce",
    "reconcile",
    "unwrap",
    "createMutable",
    "modifyMutable",
];

/// Types that should be imported from "solid-js"
const SOLID_JS_TYPES: &[&str] = &[
    "Signal",
    "Accessor",
    "Setter",
    "Resource",
    "ResourceActions",
    "ResourceOptions",
    "ResourceReturn",
    "ResourceFetcher",
    "InitializedResourceReturn",
    "Component",
    "VoidProps",
    "VoidComponent",
    "ParentProps",
    "ParentComponent",
    "FlowProps",
    "FlowComponent",
    "ValidComponent",
    "ComponentProps",
    "Ref",
    "MergeProps",
    "SplitProps",
    "Context",
    "JSX",
    "ResolvedChildren",
    "MatchProps",
];

/// Types that should be imported from "solid-js/web"
const SOLID_WEB_TYPES: &[&str] = &["MountableElement"];

/// Types that should be imported from "solid-js/store"
const SOLID_STORE_TYPES: &[&str] = &["StoreNode", "Store", "SetStoreFunction"];

/// Get the correct source for a primitive import
fn get_primitive_source(name: &str) -> Option<&'static str> {
    if SOLID_JS_PRIMITIVES.contains(&name) {
        Some("solid-js")
    } else if SOLID_WEB_PRIMITIVES.contains(&name) {
        Some("solid-js/web")
    } else if SOLID_STORE_PRIMITIVES.contains(&name) {
        Some("solid-js/store")
    } else {
        None
    }
}

/// Get the correct source for a type import
fn get_type_source(name: &str) -> Option<&'static str> {
    if SOLID_JS_TYPES.contains(&name) {
        Some("solid-js")
    } else if SOLID_WEB_TYPES.contains(&name) {
        Some("solid-js/web")
    } else if SOLID_STORE_TYPES.contains(&name) {
        Some("solid-js/store")
    } else {
        None
    }
}

/// Check if a source is a Solid source
fn is_solid_source(source: &str) -> bool {
    SOLID_SOURCES.contains(&source)
}

impl Imports {
    pub fn new() -> Self {
        Self
    }

    /// Check an import declaration for incorrect Solid imports
    pub fn check<'a>(&self, import: &ImportDeclaration<'a>) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        let source = import.source.value.as_str();

        // Only check solid-js, solid-js/web, solid-js/store imports
        if !is_solid_source(source) {
            return diagnostics;
        }

        // Check if this is a type-only import declaration
        let is_type_import = import.import_kind.is_type();

        if let Some(specifiers) = &import.specifiers {
            for specifier in specifiers {
                if let oxc_ast::ast::ImportDeclarationSpecifier::ImportSpecifier(spec) = specifier {
                    let name = spec.imported.name().as_str();

                    // Determine if this specific import is a type import
                    let is_type = is_type_import || spec.import_kind.is_type();

                    // Get the correct source for this import
                    let correct_source = if is_type {
                        get_type_source(name)
                    } else {
                        get_primitive_source(name)
                    };

                    if let Some(correct) = correct_source {
                        if correct != source {
                            diagnostics.push(
                                Diagnostic::warning(
                                    Self::NAME,
                                    spec.span,
                                    format!(
                                        "Prefer importing {} from \"{}\".",
                                        name, correct
                                    ),
                                )
                                .with_help(format!(
                                    "Import {} from \"{}\" instead of \"{}\".",
                                    name, correct, source
                                )),
                            );
                        }
                    }
                }
            }
        }

        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxc_allocator::Allocator;
    use oxc_parser::Parser;
    use oxc_span::SourceType;

    fn parse_and_get_import<'a>(
        allocator: &'a Allocator,
        source: &'a str,
    ) -> Option<oxc_ast::ast::Program<'a>> {
        let source_type = SourceType::tsx();
        let ret = Parser::new(allocator, source, source_type).parse();
        if ret.errors.is_empty() {
            Some(ret.program)
        } else {
            None
        }
    }

    fn find_import_declaration<'a>(
        program: &'a oxc_ast::ast::Program<'a>,
    ) -> Option<&'a ImportDeclaration<'a>> {
        for stmt in &program.body {
            if let oxc_ast::ast::Statement::ImportDeclaration(import) = stmt {
                return Some(import);
            }
        }
        None
    }

    #[test]
    fn test_rule_name() {
        assert_eq!(Imports::NAME, "imports");
    }

    #[test]
    fn test_correct_solid_js_import() {
        let allocator = Allocator::default();
        let source = r#"import { createSignal, createEffect } from "solid-js";"#;

        let program = parse_and_get_import(&allocator, source).expect("should parse");
        let import = find_import_declaration(&program).expect("should find import");

        let rule = Imports::new();
        let diagnostics = rule.check(import);

        assert!(diagnostics.is_empty(), "correct imports should have no diagnostics");
    }

    #[test]
    fn test_wrong_source_for_create_signal() {
        let allocator = Allocator::default();
        let source = r#"import { createSignal } from "solid-js/web";"#;

        let program = parse_and_get_import(&allocator, source).expect("should parse");
        let import = find_import_declaration(&program).expect("should find import");

        let rule = Imports::new();
        let diagnostics = rule.check(import);

        assert_eq!(diagnostics.len(), 1, "should have one diagnostic");
        assert!(diagnostics[0].message.contains("createSignal"));
        assert!(diagnostics[0].message.contains("solid-js"));
    }

    #[test]
    fn test_wrong_source_for_portal() {
        let allocator = Allocator::default();
        let source = r#"import { Portal } from "solid-js";"#;

        let program = parse_and_get_import(&allocator, source).expect("should parse");
        let import = find_import_declaration(&program).expect("should find import");

        let rule = Imports::new();
        let diagnostics = rule.check(import);

        assert_eq!(diagnostics.len(), 1, "should have one diagnostic");
        assert!(diagnostics[0].message.contains("Portal"));
        assert!(diagnostics[0].message.contains("solid-js/web"));
    }

    #[test]
    fn test_wrong_source_for_create_store() {
        let allocator = Allocator::default();
        let source = r#"import { createStore } from "solid-js";"#;

        let program = parse_and_get_import(&allocator, source).expect("should parse");
        let import = find_import_declaration(&program).expect("should find import");

        let rule = Imports::new();
        let diagnostics = rule.check(import);

        assert_eq!(diagnostics.len(), 1, "should have one diagnostic");
        assert!(diagnostics[0].message.contains("createStore"));
        assert!(diagnostics[0].message.contains("solid-js/store"));
    }

    #[test]
    fn test_correct_web_imports() {
        let allocator = Allocator::default();
        let source = r#"import { render, hydrate, Portal, Dynamic } from "solid-js/web";"#;

        let program = parse_and_get_import(&allocator, source).expect("should parse");
        let import = find_import_declaration(&program).expect("should find import");

        let rule = Imports::new();
        let diagnostics = rule.check(import);

        assert!(diagnostics.is_empty(), "correct web imports should have no diagnostics");
    }

    #[test]
    fn test_correct_store_imports() {
        let allocator = Allocator::default();
        let source = r#"import { createStore, produce, reconcile } from "solid-js/store";"#;

        let program = parse_and_get_import(&allocator, source).expect("should parse");
        let import = find_import_declaration(&program).expect("should find import");

        let rule = Imports::new();
        let diagnostics = rule.check(import);

        assert!(diagnostics.is_empty(), "correct store imports should have no diagnostics");
    }

    #[test]
    fn test_multiple_wrong_imports() {
        let allocator = Allocator::default();
        let source = r#"import { createSignal, Portal, createStore } from "solid-js";"#;

        let program = parse_and_get_import(&allocator, source).expect("should parse");
        let import = find_import_declaration(&program).expect("should find import");

        let rule = Imports::new();
        let diagnostics = rule.check(import);

        assert_eq!(diagnostics.len(), 2, "should have two diagnostics (Portal and createStore)");
    }

    #[test]
    fn test_type_import() {
        let allocator = Allocator::default();
        let source = r#"import type { Component, Accessor } from "solid-js";"#;

        let program = parse_and_get_import(&allocator, source).expect("should parse");
        let import = find_import_declaration(&program).expect("should find import");

        let rule = Imports::new();
        let diagnostics = rule.check(import);

        assert!(diagnostics.is_empty(), "correct type imports should have no diagnostics");
    }

    #[test]
    fn test_type_import_wrong_source() {
        let allocator = Allocator::default();
        let source = r#"import type { Store } from "solid-js";"#;

        let program = parse_and_get_import(&allocator, source).expect("should parse");
        let import = find_import_declaration(&program).expect("should find import");

        let rule = Imports::new();
        let diagnostics = rule.check(import);

        assert_eq!(diagnostics.len(), 1, "should have one diagnostic");
        assert!(diagnostics[0].message.contains("Store"));
        assert!(diagnostics[0].message.contains("solid-js/store"));
    }

    #[test]
    fn test_inline_type_import() {
        let allocator = Allocator::default();
        let source = r#"import { createSignal, type Component } from "solid-js";"#;

        let program = parse_and_get_import(&allocator, source).expect("should parse");
        let import = find_import_declaration(&program).expect("should find import");

        let rule = Imports::new();
        let diagnostics = rule.check(import);

        assert!(diagnostics.is_empty(), "mixed imports should have no diagnostics");
    }

    #[test]
    fn test_non_solid_import_ignored() {
        let allocator = Allocator::default();
        let source = r#"import { useState } from "react";"#;

        let program = parse_and_get_import(&allocator, source).expect("should parse");
        let import = find_import_declaration(&program).expect("should find import");

        let rule = Imports::new();
        let diagnostics = rule.check(import);

        assert!(diagnostics.is_empty(), "non-solid imports should be ignored");
    }

    #[test]
    fn test_unknown_solid_import_ignored() {
        let allocator = Allocator::default();
        let source = r#"import { unknownFunction } from "solid-js";"#;

        let program = parse_and_get_import(&allocator, source).expect("should parse");
        let import = find_import_declaration(&program).expect("should find import");

        let rule = Imports::new();
        let diagnostics = rule.check(import);

        assert!(diagnostics.is_empty(), "unknown imports should be ignored");
    }

    #[test]
    fn test_components_from_solid_js() {
        let allocator = Allocator::default();
        let source = r#"import { For, Show, Switch, Match, Index, ErrorBoundary, Suspense } from "solid-js";"#;

        let program = parse_and_get_import(&allocator, source).expect("should parse");
        let import = find_import_declaration(&program).expect("should find import");

        let rule = Imports::new();
        let diagnostics = rule.check(import);

        assert!(diagnostics.is_empty(), "control flow components should be from solid-js");
    }
}
