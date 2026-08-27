//! Verb-AL has a tiny lexicon: bare words, double-quoted identifiers,
//! single-quoted value literals, and five pieces of punctuation.

use crate::diag::{Diag, Span};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Tok {
    /// A bare word such as `local`, `1-sign-bit`, `divided-by`, `end`.
    Word(String),
    /// A double-quoted identifier, escapes already resolved.
    Ident(String),
    /// A single-quoted value literal, escapes already resolved.
    Lit(String),
    Colon,
    Comma,
    Dot,
    Equals,
    LParen,
    RParen,
    LBracket,
    RBracket,
    Eof,
}

impl Tok {
    pub fn describe(&self) -> String {
        match self {
            Tok::Word(w) => format!("`{}`", w),
            Tok::Ident(n) => format!("the name \"{}\"", n),
            Tok::Lit(v) => format!("the literal '{}'", v),
            Tok::Colon => "`:`".into(),
            Tok::Comma => "`,`".into(),
            Tok::Dot => "`.`".into(),
            Tok::Equals => "`=`".into(),
            Tok::LParen => "`(`".into(),
            Tok::RParen => "`)`".into(),
            Tok::LBracket => "`[`".into(),
            Tok::RBracket => "`]`".into(),
            Tok::Eof => "the end of the program".into(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Token {
    pub tok: Tok,
    pub span: Span,
}

pub fn lex(text: &str) -> Result<Vec<Token>, Diag> {
    let bytes: Vec<(usize, char)> = text.char_indices().collect();
    let mut i = 0usize;
    let mut out = Vec::new();

    while i < bytes.len() {
        let (start, c) = bytes[i];

        if c.is_whitespace() {
            i += 1;
            continue;
        }

        let single = match c {
            ':' => Some(Tok::Colon),
            ',' => Some(Tok::Comma),
            '.' => Some(Tok::Dot),
            '=' => Some(Tok::Equals),
            '(' => Some(Tok::LParen),
            ')' => Some(Tok::RParen),
            '[' => Some(Tok::LBracket),
            ']' => Some(Tok::RBracket),
            _ => None,
        };
        if let Some(tok) = single {
            out.push(Token { tok, span: Span::new(start, start + c.len_utf8()) });
            i += 1;
            continue;
        }

        if c == '"' || c == '\'' {
            let (value, next, end) = lex_quoted(text, &bytes, i, c)?;
            let tok = if c == '"' { Tok::Ident(value) } else { Tok::Lit(value) };
            out.push(Token { tok, span: Span::new(start, end) });
            i = next;
            continue;
        }

        if c.is_ascii_alphanumeric() {
            let mut end = start + c.len_utf8();
            let mut j = i + 1;
            while j < bytes.len() {
                let (off, ch) = bytes[j];
                // A hyphen continues a word only when a word character follows,
                // so `end-branch` is one word but a trailing hyphen is not.
                let continues = ch.is_ascii_alphanumeric()
                    || (ch == '-'
                        && bytes.get(j + 1).map_or(false, |(_, n)| n.is_ascii_alphanumeric()));
                if !continues {
                    break;
                }
                end = off + ch.len_utf8();
                j += 1;
            }
            out.push(Token { tok: Tok::Word(text[start..end].to_string()), span: Span::new(start, end) });
            i = j;
            continue;
        }

        return Err(Diag::new(
            format!("a statement is built from words, quoted runs and the punctuation `:,.=()[]`; `{}` is none of these", c),
            Span::new(start, start + c.len_utf8()),
            "remove it, or quote it — names go in \"double quotes\" and values in 'single quotes'",
        ));
    }

    let end = text.len();
    out.push(Token { tok: Tok::Eof, span: Span::new(end, end) });
    Ok(out)
}

/// Reads a quoted run starting at index `i`, returning the unescaped contents,
/// the index just past the closing quote, and its byte offset.
fn lex_quoted(
    text: &str,
    bytes: &[(usize, char)],
    i: usize,
    quote: char,
) -> Result<(String, usize, usize), Diag> {
    let (start, _) = bytes[i];
    let mut value = String::new();
    let mut j = i + 1;

    while j < bytes.len() {
        let (off, c) = bytes[j];
        if c == quote {
            return Ok((value, j + 1, off + c.len_utf8()));
        }
        if c == '\n' {
            return Err(Diag::new(
                format!("a quoted run must close on the line it opens; this {} never does", quote),
                Span::new(start, off),
                format!("add a closing {} before the end of the line", quote),
            ));
        }
        if c != '\\' {
            value.push(c);
            j += 1;
            continue;
        }

        // An escape sequence.
        let Some(&(esc_off, esc)) = bytes.get(j + 1) else {
            return Err(Diag::new(
                "a backslash must be followed by the character it escapes; this one ends the program",
                Span::new(off, text.len()),
                "complete the escape, or write `\\\\` if you meant a literal backslash",
            ));
        };
        let simple = match esc {
            '\\' => Some('\\'),
            '\'' => Some('\''),
            '"' => Some('"'),
            'n' => Some('\n'),
            't' => Some('\t'),
            'r' => Some('\r'),
            '0' => Some('\0'),
            _ => None,
        };
        if let Some(ch) = simple {
            value.push(ch);
            j += 2;
            continue;
        }
        if esc == 'u' {
            // \u{XXXX}
            let mut k = j + 2;
            if bytes.get(k).map(|&(_, c)| c) != Some('{') {
                return Err(Diag::new(
                    "a \\u escape names its scalar in braces; the brace is missing",
                    Span::new(off, esc_off + 1),
                    "write it as \\u{...}, for instance \\u{1F923}",
                ));
            }
            k += 1;
            let mut digits = String::new();
            while let Some(&(_, c)) = bytes.get(k) {
                if c == '}' {
                    break;
                }
                digits.push(c);
                k += 1;
            }
            let close = bytes.get(k).map(|&(o, c)| (o, c));
            let Some((close_off, '}')) = close else {
                return Err(Diag::new(
                    "a \\u escape closes with `}`; this one never does",
                    Span::new(off, text.len().min(off + 16)),
                    "add a closing `}` after the hexadecimal digits",
                ));
            };
            let scalar = u32::from_str_radix(&digits, 16)
                .ok()
                .and_then(char::from_u32)
                .ok_or_else(|| {
                    Diag::new(
                        format!(
                            "a \\u escape names a Unicode scalar; `{}` is not one",
                            digits
                        ),
                        Span::new(off, close_off + 1),
                        "use hexadecimal digits below 110000, skipping D800 through DFFF",
                    )
                })?;
            value.push(scalar);
            j = k + 1;
            continue;
        }
        return Err(Diag::new(
            format!("`\\{}` is not one of the escapes Verb-AL knows", esc),
            Span::new(off, esc_off + esc.len_utf8()),
            "use one of \\\\ \\' \\\" \\n \\t \\r \\0 \\u{...}, or write `\\\\` for a literal backslash",
        ));
    }

    Err(Diag::new(
        "a quoted run must be closed; this one runs to the end of the program",
        Span::new(start, text.len()),
        format!("add a closing {}", quote),
    ))
}

/// Lex as much as can be lexed, ignoring anything unrecognisable.
///
/// The permission scan runs before the compiler is allowed to say anything, so
/// it cannot report a lexical error — it just reads past one. A file whose
/// opt-in is intact still gets its diagnostics even when the text after it does
/// not lex.
pub fn lex_lossy(text: &str) -> Vec<Token> {
    let mut from = 0usize;
    let mut out = Vec::new();
    loop {
        match lex(&text[from..]) {
            Ok(tokens) => {
                out.extend(shifted(tokens, from));
                return out;
            }
            Err(diag) => {
                let Some(span) = diag.span else { return out };
                let partial = lex(&text[from..span.start]).unwrap_or_default();
                out.extend(shifted(partial, from));
                // Step past the offending character and keep going.
                let next = from + span.end.max(span.start + 1);
                if next >= text.len() || !text.is_char_boundary(next) {
                    return out;
                }
                from = next;
            }
        }
    }
}

fn shifted(tokens: Vec<Token>, by: usize) -> impl Iterator<Item = Token> {
    tokens.into_iter().filter(|t| t.tok != Tok::Eof).map(move |mut t| {
        t.span = Span::new(t.span.start + by, t.span.end + by);
        t
    })
}
