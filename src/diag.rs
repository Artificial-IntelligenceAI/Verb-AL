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

#[derive(Clone, Debug)]
pub struct Diag {
    pub msg: String,
    pub span: Option<Span>,
    pub notes: Vec<String>,
}

impl Diag {
    pub fn new(msg: impl Into<String>, span: Span) -> Self {
        Diag { msg: msg.into(), span: Some(span), notes: Vec::new() }
    }
    pub fn bare(msg: impl Into<String>) -> Self {
        Diag { msg: msg.into(), span: None, notes: Vec::new() }
    }
    pub fn note(mut self, note: impl Into<String>) -> Self {
        self.notes.push(note.into());
        self
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

    fn line_text(&self, line: usize) -> &str {
        self.text.lines().nth(line - 1).unwrap_or("")
    }

    pub fn render(&self, d: &Diag) -> String {
        let mut out = format!("error: {}\n", d.msg);
        if let Some(span) = d.span {
            let (line, col) = self.line_col(span.start);
            let text = self.line_text(line);
            let gutter = line.to_string().len();
            out.push_str(&format!("{:>w$}--> {}:{}:{}\n", "", self.path, line, col, w = gutter));
            out.push_str(&format!("{:>w$} |\n", "", w = gutter));
            out.push_str(&format!("{} | {}\n", line, text));
            // Underline as many characters as the span covers on this line.
            let end_on_line = span.end.min(
                self.text[span.start..].find('\n').map(|i| span.start + i).unwrap_or(self.text.len()),
            );
            let width = self.text[span.start..end_on_line].chars().count().max(1);
            out.push_str(&format!(
                "{:>w$} | {}{}\n",
                "",
                " ".repeat(col - 1),
                "^".repeat(width),
                w = gutter
            ));
            for note in &d.notes {
                out.push_str(&format!("{:>w$} = {}\n", "", note, w = gutter));
            }
        } else {
            for note in &d.notes {
                out.push_str(&format!("  = {}\n", note));
            }
        }
        out
    }
}
