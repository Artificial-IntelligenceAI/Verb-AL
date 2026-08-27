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
    /// `requires:target.64-bit-pointers.little-endian.8-byte-maximum-alignmentend`
    ///
    /// What the program needs of the machine it is built for. Only what the
    /// source can actually depend on: the CPU and its features change nothing
    /// a program is allowed to claim, so they belong to the build, not here.
    Requires {
        pointer_bits: u32,
        little_endian: bool,
        max_alignment: u32,
        span: Span,
    },

    /// `allow[compiler:error.error-message]end` — what the program permits the
    /// compiler to do. Compile-time only; it runs no code.
    Allow { permission: String, permission_span: Span, span: Span },
    Note { remark: String, span: Span },
    /// `standard-output:print.<classes>.end:[…]end`
    ///
    /// The descriptor permits the character classes the *literal* content may
    /// draw on, exactly as a name descriptor does for a name.
    Print {
        classes: Vec<CharClass>,
        /// Whether the descriptor said `newline-too`. A write ends with a
        /// newline only when it says so.
        newline: bool,
        /// Whether the descriptor said `variable`, permitting the write to
        /// name one.
        variable: bool,
        classes_span: Span,
        items: Vec<PrintItem>,
        span: Span,
    },
    Assign { target: String, target_span: Span, value: Expr, span: Span },
    Branch { cond: Expr, then: Vec<Stmt>, otherwise: Vec<Stmt>, span: Span },
    Repeat { cond: Expr, body: Vec<Stmt>, span: Span },
}

/// One thing to be written, the parts joined by `connect with`.
///
/// Literal character content is class-checked. A variable is named by
/// restating its declaration in full: at the point of use, the program says
/// again exactly what it is using.
#[derive(Clone, Debug)]
pub enum PrintItem {
    Literal { text: String, span: Span },
    Variable(Box<Decl>),
}

/// Whether two initializers are the same expression, disregarding where in the
/// file each was written.
pub fn same_expr(a: &Expr, b: &Expr) -> bool {
    match (a, b) {
        (Expr::Ident { name: x, .. }, Expr::Ident { name: y, .. }) => x == y,
        (Expr::Lit { raw: x, .. }, Expr::Lit { raw: y, .. }) => x == y,
        (
            Expr::Unary { op: p, operand: x, .. },
            Expr::Unary { op: q, operand: y, .. },
        ) => p == q && same_expr(x, y),
        (
            Expr::Binary { op: p, lhs: xl, rhs: xr, .. },
            Expr::Binary { op: q, lhs: yl, rhs: yr, .. },
        ) => p == q && same_expr(xl, yl) && same_expr(xr, yr),
        _ => false,
    }
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
