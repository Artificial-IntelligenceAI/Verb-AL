//! Verb-AL: a language in which nothing is implicit.

pub mod ast;
pub mod check;
pub mod codegen;
pub mod diag;
pub mod fmt;
pub mod interp;
pub mod lexer;
pub mod machine;
pub mod parser;
pub mod permission;
pub mod tast;
pub mod types;
pub mod value;

use diag::{Diag, Source};
use tast::Program;

/// A program that would not compile. `rendered` holds the diagnostic — unless
/// the program never permitted the compiler to produce one, in which case the
/// failure is silent and only the exit status says anything.
pub struct Rejection {
    pub rendered: Option<String>,
}

/// Lex, parse and check.
///
/// Before any of that, read the program's permissions: whether the compiler is
/// allowed to explain itself is itself a fact the program must state, so it has
/// to be known before the first thing that could go wrong.
pub fn front_end(source: &Source, machine: &machine::Machine) -> Result<Program, Rejection> {
    let grants = permission::scan(&lexer::lex_lossy(&source.text));
    let may_speak = grants.allows(permission::ERROR_MESSAGE);
    let reject = |d: Diag| Rejection {
        rendered: may_speak.then(|| source.render(&d)),
    };

    let tokens = lexer::lex(&source.text).map_err(reject)?;
    let stmts = parser::parse(&tokens).map_err(reject)?;
    check::check(&stmts, machine).map_err(reject)
}
