//! Lint context for rule execution

use oxc_semantic::Semantic;
use oxc_span::SourceType;

use crate::Diagnostic;

/// Context passed to rules during linting
pub struct LintContext<'a> {
    /// Source code being linted
    source_text: &'a str,
    /// Source type (JS/TS/JSX etc)
    source_type: SourceType,
    /// Semantic analysis (scopes, symbols, etc.)
    semantic: Option<&'a Semantic<'a>>,
    /// Collected diagnostics
    diagnostics: Vec<Diagnostic>,
}

impl<'a> LintContext<'a> {
    pub fn new(source_text: &'a str, source_type: SourceType) -> Self {
        Self {
            source_text,
            source_type,
            semantic: None,
            diagnostics: Vec::new(),
        }
    }

    pub fn with_semantic(mut self, semantic: &'a Semantic<'a>) -> Self {
        self.semantic = Some(semantic);
        self
    }

    /// Get the source text
    pub fn source_text(&self) -> &'a str {
        self.source_text
    }

    /// Get the source type
    pub fn source_type(&self) -> SourceType {
        self.source_type
    }

    /// Check if the source is JSX
    pub fn is_jsx(&self) -> bool {
        self.source_type.is_jsx()
    }

    /// Check if the source is TypeScript
    pub fn is_typescript(&self) -> bool {
        self.source_type.is_typescript()
    }

    /// Get semantic analysis if available
    pub fn semantic(&self) -> Option<&'a Semantic<'a>> {
        self.semantic
    }

    /// Report a diagnostic
    pub fn report(&mut self, diagnostic: Diagnostic) {
        self.diagnostics.push(diagnostic);
    }

    /// Get a slice of source text for a span
    pub fn span_text(&self, span: oxc_span::Span) -> &'a str {
        &self.source_text[span.start as usize..span.end as usize]
    }

    /// Consume the context and return all diagnostics
    pub fn into_diagnostics(self) -> Vec<Diagnostic> {
        self.diagnostics
    }

    /// Get reference to diagnostics
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }
}
