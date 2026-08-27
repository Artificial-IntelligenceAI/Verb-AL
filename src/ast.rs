//! The parse tree: faithful to what was written, before any checking.

use crate::diag::Span;
use crate::types::{CharClass, Type};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Privacy {
    Local,
    Public,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MemoryClass {
    Static,
    Automatic,
}

#[derive(Clone, Debug)]
pub struct Decl {
    pub privacy: Privacy,
    pub memory: MemoryClass,
    pub ty: Type,
    pub ty_span: Span,
    pub name_classes: Vec<CharClass>,
    pub name_desc_span: Span,
    pub name: String,
    pub name_span: Span,
    pub init: Expr,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub enum Stmt {
    Decl(Decl),
    Note { remark: String, span: Span },
    Say { source: Expr, span: Span },
    Assign { target: String, target_span: Span, value: Expr, span: Span },
    Branch { cond: Expr, then: Vec<Stmt>, otherwise: Vec<Stmt>, span: Span },
    Repeat { cond: Expr, body: Vec<Stmt>, span: Span },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnOp {
    Not,
    Negated,
}

impl UnOp {
    pub fn word(self) -> &'static str {
        match self {
            UnOp::Not => "not",
            UnOp::Negated => "negated",
        }
    }
    pub fn from_word(w: &str) -> Option<UnOp> {
        match w {
            "not" => Some(UnOp::Not),
            "negated" => Some(UnOp::Negated),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinOp {
    Plus,
    Minus,
    Times,
    DividedBy,
    RemainderOf,
    EqualTo,
    NotEqualTo,
    LessThan,
    GreaterThan,
    AtLeast,
    AtMost,
    And,
    Or,
    JoinedWith,
}

impl BinOp {
    pub fn word(self) -> &'static str {
        use BinOp::*;
        match self {
            Plus => "plus",
            Minus => "minus",
            Times => "times",
            DividedBy => "divided-by",
            RemainderOf => "remainder-of",
            EqualTo => "equal-to",
            NotEqualTo => "not-equal-to",
            LessThan => "less-than",
            GreaterThan => "greater-than",
            AtLeast => "at-least",
            AtMost => "at-most",
            And => "and",
            Or => "or",
            JoinedWith => "joined-with",
        }
    }

    pub fn from_word(w: &str) -> Option<BinOp> {
        use BinOp::*;
        Some(match w {
            "plus" => Plus,
            "minus" => Minus,
            "times" => Times,
            "divided-by" => DividedBy,
            "remainder-of" => RemainderOf,
            "equal-to" => EqualTo,
            "not-equal-to" => NotEqualTo,
            "less-than" => LessThan,
            "greater-than" => GreaterThan,
            "at-least" => AtLeast,
            "at-most" => AtMost,
            "and" => And,
            "or" => Or,
            "joined-with" => JoinedWith,
            _ => return None,
        })
    }

    pub fn yields_truth(self) -> bool {
        use BinOp::*;
        matches!(self, EqualTo | NotEqualTo | LessThan | GreaterThan | AtLeast | AtMost | And | Or)
    }
}

#[derive(Clone, Debug)]
pub enum Expr {
    /// A reference to a declared name.
    Ident { name: String, span: Span },
    /// A value literal, still raw text: its type comes from context.
    Lit { raw: String, span: Span },
    Unary { op: UnOp, operand: Box<Expr>, span: Span },
    Binary { op: BinOp, lhs: Box<Expr>, rhs: Box<Expr>, op_span: Span, span: Span },
}

impl Expr {
    pub fn span(&self) -> Span {
        match self {
            Expr::Ident { span, .. }
            | Expr::Lit { span, .. }
            | Expr::Unary { span, .. }
            | Expr::Binary { span, .. } => *span,
        }
    }
}
