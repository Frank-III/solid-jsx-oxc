//! Solid-specific lint rules
//!
//! This crate provides lint rules for Solid.js ported from eslint-plugin-solid.
//! Rules can be used:
//! 1. Standalone with oxc AST for custom tooling
//! 2. Integrated with oxlint as a plugin (future)
//! 3. With type-aware analysis via tsgolint integration (future)

pub mod rules;
pub mod utils;
pub mod visitor;
mod context;
mod diagnostic;

pub use context::LintContext;
pub use diagnostic::{Diagnostic, DiagnosticSeverity, Fix};
pub use rules::*;
pub use visitor::{lint, lint_with_config, LintResult, LintRunner, RulesConfig, VisitorLintContext};

/// Rule category for Solid rules
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleCategory {
    /// Rules that detect code that is likely to be incorrect
    Correctness,
    /// Rules that suggest improvements
    Pedantic,
    /// Rules that encourage best practices
    Style,
    /// Rules that may have false positives (experimental)
    Nursery,
}

/// Rule metadata
pub trait RuleMeta {
    const NAME: &'static str;
    const CATEGORY: RuleCategory;
    /// URL to documentation
    fn docs_url() -> String {
        format!(
            "https://github.com/solidjs-community/eslint-plugin-solid/blob/main/docs/{}.md",
            Self::NAME
        )
    }
}
