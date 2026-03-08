//! Lint context for rule execution
//!
//! This module provides the shared context for all lint rules, including:
//! - Source text and type information
//! - Semantic analysis (scopes, symbols, etc.)
//! - Symbol tracking (used variables, component detection)

use oxc_semantic::{ScopeId, Scoping, Semantic, SymbolId};
use oxc_span::SourceType;
use rustc_hash::FxHashSet;

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
    /// Symbols that have been used (for jsx-uses-vars)
    symbols_used: FxHashSet<SymbolId>,
    /// Symbol IDs that are known to be components (used in JSX or PascalCase + JSX return)
    component_symbols: FxHashSet<SymbolId>,
    /// Names imported from solid-js (for heuristic detection)
    solid_imports: FxHashSet<String>,
}

impl<'a> LintContext<'a> {
    pub fn new(source_text: &'a str, source_type: SourceType) -> Self {
        Self {
            source_text,
            source_type,
            semantic: None,
            diagnostics: Vec::new(),
            symbols_used: FxHashSet::default(),
            component_symbols: FxHashSet::default(),
            solid_imports: FxHashSet::default(),
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

    /// Get scoping from semantic if available
    pub fn scoping(&self) -> Option<&Scoping> {
        self.semantic.map(|s| s.scoping())
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

    // ==================== Symbol tracking (Phase 2) ====================

    /// Resolve an identifier in the given scope, returning its SymbolId if found
    pub fn resolve_in_scope(&self, scope_id: ScopeId, name: &str) -> Option<SymbolId> {
        self.scoping()?.find_binding(scope_id, name.into())
    }

    /// Resolve an identifier starting from the given scope and walking up
    pub fn resolve_binding(&self, scope_id: ScopeId, name: &str) -> Option<SymbolId> {
        self.scoping()?.find_binding(scope_id, name.into())
    }

    /// Check if a binding exists for the given name in scope
    pub fn is_defined(&self, scope_id: ScopeId, name: &str) -> bool {
        self.resolve_in_scope(scope_id, name).is_some()
    }

    /// Mark a symbol as used (for jsx-uses-vars)
    pub fn mark_used(&mut self, symbol_id: SymbolId) {
        self.symbols_used.insert(symbol_id);
    }

    /// Check if a symbol is marked as used
    pub fn is_used(&self, symbol_id: SymbolId) -> bool {
        self.symbols_used.contains(&symbol_id)
    }

    /// Get all used symbols
    pub fn used_symbols(&self) -> &FxHashSet<SymbolId> {
        &self.symbols_used
    }

    /// Mark a symbol as a component
    pub fn mark_component(&mut self, symbol_id: SymbolId) {
        self.component_symbols.insert(symbol_id);
    }

    /// Check if a symbol is a known component
    pub fn is_component(&self, symbol_id: SymbolId) -> bool {
        self.component_symbols.contains(&symbol_id)
    }

    /// Get all component symbols
    pub fn component_symbols(&self) -> &FxHashSet<SymbolId> {
        &self.component_symbols
    }

    /// Register a Solid import (e.g., "createSignal", "createMemo")
    pub fn register_solid_import(&mut self, name: String) {
        self.solid_imports.insert(name);
    }

    /// Check if a name is a known Solid import
    pub fn is_solid_import(&self, name: &str) -> bool {
        self.solid_imports.contains(name)
    }

    /// Get all registered Solid imports
    pub fn solid_imports(&self) -> &FxHashSet<String> {
        &self.solid_imports
    }
}
