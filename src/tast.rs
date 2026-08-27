//! The checked tree. Every type is resolved, every literal is a value, and
//! every name is a slot index, so the backends have no decisions left to make.

use crate::ast::{BinOp, Privacy, UnOp};
use crate::diag::Span;
use crate::types::Type;
use crate::value::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    /// One cell for the whole program run.
    Static(usize),
    /// A cell belonging to the block that declared it.
    Auto(usize),
}

#[derive(Clone, Debug)]
pub enum TExpr {
    Read { place: Place, ty: Type },
    Const { value: Value, ty: Type },
    Unary { op: UnOp, ty: Type, operand: Box<TExpr> },
    Binary { op: BinOp, ty: Type, operand_ty: Type, lhs: Box<TExpr>, rhs: Box<TExpr>, span: Span },
}

impl TExpr {
    pub fn ty(&self) -> Type {
        match self {
            TExpr::Read { ty, .. }
            | TExpr::Const { ty, .. }
            | TExpr::Unary { ty, .. }
            | TExpr::Binary { ty, .. } => *ty,
        }
    }
}

#[derive(Clone, Debug)]
pub enum TStmt {
    /// Reached each time control passes the declaration.
    InitAuto { slot: usize, ty: Type, init: TExpr },
    /// Write each part in order, then a newline.
    Print { parts: Vec<TExpr> },
    Assign { place: Place, ty: Type, value: TExpr },
    Branch { cond: TExpr, then: Vec<TStmt>, otherwise: Vec<TStmt> },
    Repeat { cond: TExpr, body: Vec<TStmt> },
}

#[derive(Clone, Debug)]
pub struct StaticVar {
    pub name: String,
    pub ty: Type,
    pub privacy: Privacy,
    /// Folded at compile time, so the compiler can emit a real initializer.
    pub init: Value,
}

#[derive(Clone, Debug)]
pub struct AutoVar {
    pub name: String,
    pub ty: Type,
}

#[derive(Clone, Debug)]
pub struct Program {
    pub statics: Vec<StaticVar>,
    pub autos: Vec<AutoVar>,
    pub body: Vec<TStmt>,
}
