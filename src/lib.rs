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

/// Whether this program has permitted the compiler to explain itself. Also
/// governs what is said about the machine file it was built with: the program
/// is what asked to be told.
pub fn may_report(source: &Source) -> bool {
    permission::scan(&lexer::lex_lossy(&source.text)).allows(permission::ERROR_MESSAGE)
}

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
    let may_speak = may_report(source);
    let reject = |d: Diag| Rejection {
        rendered: may_speak.then(|| source.render(&d)),
    };

    let tokens = lexer::lex(&source.text).map_err(reject)?;
    let stmts = parser::parse(&tokens).map_err(reject)?;
    check::check(&stmts, machine).map_err(reject)
}

/// Read and check a `.machine` file. Its diagnostics are gated by the
/// program's permission, since the program is what asked to be told.
pub fn machine_file(path: &str, may_speak: bool) -> Result<machine::Built, Rejection> {
    let quiet = || Rejection { rendered: None };
    let text = std::fs::read_to_string(path).map_err(|_| quiet())?;
    let source = Source::new(path.to_string(), text);
    let reject = |d: Diag| Rejection { rendered: may_speak.then(|| source.render(&d)) };

    let tokens = lexer::lex(&source.text).map_err(reject)?;
    let spec = parser::parse_machine(&tokens).map_err(reject)?;
    machine::from_spec(&spec).map_err(reject)
}
