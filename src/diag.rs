//! Source positions and human-readable error rendering.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Self {
        Span { start, end }
    }
    pub fn to(self, other: Span) -> Span {
        Span::new(self.start.min(other.start), self.end.max(other.end))
    }
}

/// A broken rule, in the shape Verb-AL reports it: what rule broke and where,
/// and what to write instead. Both are required at construction — a diagnostic
/// that cannot suggest a fix is not finished being written.
#[derive(Clone, Debug)]
pub struct Diag {
    pub rule: String,
    pub fix: String,
    pub span: Option<Span>,
}

impl Diag {
    pub fn new(rule: impl Into<String>, span: Span, fix: impl Into<String>) -> Self {
        Diag { rule: rule.into(), fix: fix.into(), span: Some(span) }
    }

    /// Add a further clause to the suggested fix.
    pub fn also(mut self, extra: impl AsRef<str>) -> Self {
        self.fix = format!("{}; {}", self.fix, extra.as_ref());
        self
    }

    /// For the rare fault that belongs to no particular stretch of text.
    pub fn unplaced(rule: impl Into<String>, fix: impl Into<String>) -> Self {
        Diag { rule: rule.into(), fix: fix.into(), span: None }
    }
}

/// A named blob of source text, able to turn byte offsets back into
/// line/column positions and render a diagnostic against them.
pub struct Source {
    pub path: String,
    pub text: String,
}

impl Source {
    pub fn new(path: impl Into<String>, text: impl Into<String>) -> Self {
        Source { path: path.into(), text: text.into() }
    }

    /// One-based line and column (column counted in characters, not bytes).
    pub fn line_col(&self, offset: usize) -> (usize, usize) {
        let offset = offset.min(self.text.len());
        let before = &self.text[..offset];
        let line = before.matches('\n').count() + 1;
        let line_start = before.rfind('\n').map(|i| i + 1).unwrap_or(0);
        let col = self.text[line_start..offset].chars().count() + 1;
        (line, col)
    }

    /// The report format: a location a terminal will linkify, then the same
    /// facts spelled out one per line, because Verb-AL leaves nothing implicit
    /// and that includes what a diagnostic is telling you.
    pub fn render(&self, d: &Diag) -> String {
        let mut out = String::new();
        match d.span {
            Some(span) => {
                let (line, column) = self.line_col(span.start);
                out.push_str(&format!("{}:{}:{}\n", self.path, line, column));
                out.push_str(&format!("file: {}\n", self.path));
                out.push_str(&format!("line: {}\n", line));
                out.push_str(&format!("column: {}\n", column));
            }
            None => {
                out.push_str(&format!("{}\n", self.path));
                out.push_str(&format!("file: {}\n", self.path));
                out.push_str("line: (the whole file)\n");
                out.push_str("column: (the whole file)\n");
            }
        }
        out.push_str(&format!("rule broke & where: {}\n", d.rule));
        out.push_str(&format!("suggested fix: {}\n", d.fix));
        out
    }
}
