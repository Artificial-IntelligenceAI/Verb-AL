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
    /// Every report names a place. A rule about the whole program is broken at
    /// its beginning, so that even the absence of a statement has a location
    /// and the report keeps its one shape.
    pub span: Span,
}

impl Diag {
    pub fn new(rule: impl Into<String>, span: Span, fix: impl Into<String>) -> Self {
        Diag { rule: rule.into(), fix: fix.into(), span }
    }

    /// Add a further clause to the suggested fix.
    pub fn also(mut self, extra: impl AsRef<str>) -> Self {
        self.fix = format!("{}; {}", self.fix, extra.as_ref());
        self
    }

}

/// Text from the program, made safe to quote inside a report: a report is six
/// lines, and a literal newline in a name or a written run would make it more.
pub fn showable(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            '\n' => "\\n".to_string(),
            '\t' => "\\t".to_string(),
            '\r' => "\\r".to_string(),
            '\0' => "\\0".to_string(),
            c if (c as u32) < 0x20 => format!("\\u{{{:X}}}", c as u32),
            c => c.to_string(),
        })
        .collect()
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
        let (line, column) = self.line_col(d.span.start);
        let mut out = String::new();
        out.push_str(&format!("{}:{}:{}\n", self.path, line, column));
        out.push_str(&format!("file: {}\n", self.path));
        out.push_str(&format!("line: {}\n", line));
        out.push_str(&format!("column: {}\n", column));
        out.push_str(&format!("rule broke & where: {}\n", d.rule));
        out.push_str(&format!("suggested fix: {}\n", d.fix));
        out
    }
}
