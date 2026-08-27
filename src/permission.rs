//! Verb-AL leaves nothing implicit, including whether the compiler may speak.
//!
//! A program that has not permitted error messages does not get error messages:
//! it fails silently, with a status and nothing else. Permission is granted by
//! an `allow` statement:
//!
//! ```text
//! allow[compiler:error.error-message]end
//! ```
//!
//! The scan below runs before anything else, on a lossy lex, because the
//! compiler must know whether it is allowed to complain before it discovers
//! that it wants to.

use crate::lexer::{Tok, Token};

/// Permission to report a compile error at all.
pub const ERROR_MESSAGE: &str = "compiler:error.error-message";

/// Every permission a program may grant. A grant covers itself and everything
/// beneath it, so `compiler:error` also permits `compiler:error.error-message`.
pub const KNOWN: &[&str] = &["compiler:error", ERROR_MESSAGE];

#[derive(Debug, Default, Clone)]
pub struct Grants {
    granted: Vec<String>,
}

impl Grants {
    pub fn allows(&self, wanted: &str) -> bool {
        self.granted.iter().any(|g| covers(g, wanted))
    }
}

/// `compiler:error` covers `compiler:error.error-message`, but `compiler:err`
/// covers nothing — a grant may only end at a dot.
fn covers(granted: &str, wanted: &str) -> bool {
    wanted == granted
        || wanted.strip_prefix(granted).is_some_and(|rest| rest.starts_with('.'))
}

/// Collect every `allow[subject:path]` in a token stream, however broken the
/// rest of the stream is. Permissions the compiler does not recognise are left
/// for the parser and checker to complain about — if they are allowed to.
pub fn scan(tokens: &[Token]) -> Grants {
    let mut granted = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        if tokens[i].tok != Tok::Word("allow".into()) {
            i += 1;
            continue;
        }
        if let Some((permission, next)) = read_permission(tokens, i + 1) {
            if KNOWN.contains(&permission.as_str()) {
                granted.push(permission);
            }
            i = next;
        } else {
            i += 1;
        }
    }
    Grants { granted }
}

/// `[subject:head.tail…]`, starting at the bracket.
fn read_permission(tokens: &[Token], mut i: usize) -> Option<(String, usize)> {
    if tokens.get(i)?.tok != Tok::LBracket {
        return None;
    }
    i += 1;
    let Tok::Word(subject) = &tokens.get(i)?.tok else { return None };
    i += 1;
    if tokens.get(i)?.tok != Tok::Colon {
        return None;
    }
    i += 1;
    let Tok::Word(head) = &tokens.get(i)?.tok else { return None };
    i += 1;
    let mut path = head.clone();
    while tokens.get(i).map(|t| &t.tok) == Some(&Tok::Dot) {
        let Tok::Word(part) = &tokens.get(i + 1)?.tok else { return None };
        path.push('.');
        path.push_str(part);
        i += 2;
    }
    if tokens.get(i)?.tok != Tok::RBracket {
        return None;
    }
    Some((format!("{}:{}", subject, path), i + 1))
}
