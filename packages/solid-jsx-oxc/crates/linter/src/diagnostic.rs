//! Diagnostic types for lint results

use oxc_span::Span;

/// Severity level for diagnostics
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticSeverity {
    Error,
    Warning,
    Info,
    Hint,
}

/// A suggested fix for a diagnostic
#[derive(Debug, Clone)]
pub struct Fix {
    /// Start position of the span to replace
    pub start: u32,
    /// End position of the span to replace
    pub end: u32,
    /// The replacement text
    pub replacement: String,
    /// Description of what the fix does
    pub message: Option<String>,
}

impl Fix {
    pub fn new(span: Span, replacement: impl Into<String>) -> Self {
        Self {
            start: span.start,
            end: span.end,
            replacement: replacement.into(),
            message: None,
        }
    }

    pub fn with_message(mut self, message: impl Into<String>) -> Self {
        self.message = Some(message.into());
        self
    }

    pub fn span(&self) -> Span {
        Span::new(self.start, self.end)
    }
}

/// A lint diagnostic
#[derive(Debug, Clone)]
pub struct Diagnostic {
    /// The rule that produced this diagnostic
    pub rule: String,
    /// Start position of the span
    pub start: u32,
    /// End position of the span
    pub end: u32,
    /// Primary message
    pub message: String,
    /// Optional help text
    pub help: Option<String>,
    /// Severity level
    pub severity: DiagnosticSeverity,
    /// Optional labels pointing to related locations
    pub labels: Vec<(u32, u32, String)>,
    /// Suggested fixes
    pub fixes: Vec<Fix>,
}

impl Diagnostic {
    pub fn new(rule: impl Into<String>, span: Span, message: impl Into<String>) -> Self {
        Self {
            rule: rule.into(),
            start: span.start,
            end: span.end,
            message: message.into(),
            help: None,
            severity: DiagnosticSeverity::Warning,
            labels: Vec::new(),
            fixes: Vec::new(),
        }
    }

    pub fn span(&self) -> Span {
        Span::new(self.start, self.end)
    }

    pub fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }

    pub fn with_severity(mut self, severity: DiagnosticSeverity) -> Self {
        self.severity = severity;
        self
    }

    pub fn with_label(mut self, span: Span, message: impl Into<String>) -> Self {
        self.labels.push((span.start, span.end, message.into()));
        self
    }

    pub fn with_fix(mut self, fix: Fix) -> Self {
        self.fixes.push(fix);
        self
    }

    pub fn error(rule: impl Into<String>, span: Span, message: impl Into<String>) -> Self {
        Self::new(rule, span, message).with_severity(DiagnosticSeverity::Error)
    }

    pub fn warning(rule: impl Into<String>, span: Span, message: impl Into<String>) -> Self {
        Self::new(rule, span, message).with_severity(DiagnosticSeverity::Warning)
    }
}
