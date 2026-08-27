//! Verb-AL: a language in which nothing is implicit.

pub mod ast;
pub mod check;
pub mod codegen;
pub mod diag;
pub mod fmt;
pub mod interp;
pub mod lexer;
pub mod parser;
pub mod tast;
pub mod types;
pub mod value;

use diag::Source;
use tast::Program;

/// Lex, parse and check, reporting any failure as text ready to print.
pub fn front_end(source: &Source) -> Result<Program, String> {
    let tokens = lexer::lex(&source.text).map_err(|d| source.render(&d))?;
    let stmts = parser::parse(&tokens).map_err(|d| source.render(&d))?;
    check::check(&stmts).map_err(|d| source.render(&d))
}
