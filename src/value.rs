//! Runtime values and the operator semantics shared by every backend.
//!
//! The interpreter evaluates with these functions directly; the compiler emits
//! LLVM that reproduces them; the checker uses them to fold the constant
//! initializers of static variables. One definition, three consumers, so the
//! backends cannot drift apart.

use crate::ast::{BinOp, UnOp};
use crate::types::{FloatKind, Type};

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    /// Always normalised to its declared width and signedness.
    Int(i128),
    /// Always rounded to its declared layout.
    Float(f64),
    Truth(bool),
    Char(char),
    Text(String),
}

/// The ways a Verb-AL program can stop early. Each carries a fixed message, so
/// the interpreter and the compiled program report a fault in the same bytes
/// rather than in two hand-written approximations of each other.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Fault {
    DivideByZero,
    RemainderByZero,
    DivideOverflow,
}

impl Fault {
    pub fn message(self) -> &'static str {
        match self {
            Fault::DivideByZero => "verb-al: a value was divided by zero\n",
            Fault::RemainderByZero => "verb-al: a remainder was taken by zero\n",
            Fault::DivideOverflow => {
                "verb-al: dividing the least value by minus one does not fit\n"
            }
        }
    }

    pub const ALL: [Fault; 3] =
        [Fault::DivideByZero, Fault::RemainderByZero, Fault::DivideOverflow];
}

#[derive(Clone, Copy, Debug)]
pub struct RuntimeError {
    pub fault: Fault,
}

/// The status a faulting program exits with.
pub const FAULT_EXIT: i32 = 3;

pub type Outcome = Result<Value, RuntimeError>;

/// Wrap an integer into its declared width, exactly as LLVM's `add`/`sub`/`mul`
/// do when emitted without `nsw`/`nuw`.
pub fn normalize_int(v: i128, width: u32, signed: bool) -> i128 {
    debug_assert!((1..=64).contains(&width));
    let mask: i128 = if width == 128 { -1 } else { (1i128 << width) - 1 };
    let truncated = v & mask;
    if signed && (truncated >> (width - 1)) & 1 == 1 {
        truncated - (1i128 << width)
    } else {
        truncated
    }
}

pub fn min_int(width: u32, signed: bool) -> i128 {
    if signed {
        -(1i128 << (width - 1))
    } else {
        0
    }
}

pub fn unary(op: UnOp, ty: Type, operand: &Value) -> Outcome {
    match (op, operand) {
        (UnOp::Not, Value::Truth(b)) => Ok(Value::Truth(!b)),
        (UnOp::Negated, Value::Int(v)) => {
            let Type::Int { width, signed } = ty else { unreachable!() };
            Ok(Value::Int(normalize_int(-v, width, signed)))
        }
        (UnOp::Negated, Value::Float(v)) => {
            let Type::Float(k) = ty else { unreachable!() };
            Ok(Value::Float(k.round(-v)))
        }
        _ => unreachable!("the checker rejects every other combination"),
    }
}

pub fn binary(op: BinOp, operand_ty: Type, lhs: &Value, rhs: &Value) -> Outcome {
    use BinOp::*;
    match operand_ty {
        Type::Int { width, signed } => {
            let (Value::Int(a), Value::Int(b)) = (lhs, rhs) else { unreachable!() };
            let (a, b) = (*a, *b);
            let wrap = |v: i128| Ok(Value::Int(normalize_int(v, width, signed)));
            match op {
                Plus => wrap(a.wrapping_add(b)),
                Minus => wrap(a.wrapping_sub(b)),
                Times => wrap(a.wrapping_mul(b)),
                DividedBy => {
                    check_divisor(a, b, width, signed, Fault::DivideByZero)?;
                    wrap(a / b)
                }
                RemainderOf => {
                    check_divisor(a, b, width, signed, Fault::RemainderByZero)?;
                    wrap(a % b)
                }
                EqualTo => Ok(Value::Truth(a == b)),
                NotEqualTo => Ok(Value::Truth(a != b)),
                LessThan => Ok(Value::Truth(a < b)),
                GreaterThan => Ok(Value::Truth(a > b)),
                AtLeast => Ok(Value::Truth(a >= b)),
                AtMost => Ok(Value::Truth(a <= b)),
                And | Or | JoinedWith => unreachable!(),
            }
        }

        Type::Float(kind) => {
            let (Value::Float(a), Value::Float(b)) = (lhs, rhs) else { unreachable!() };
            let (a, b) = (*a, *b);
            let round = |v: f64| Ok(Value::Float(kind.round(v)));
            match op {
                Plus => round(a + b),
                Minus => round(a - b),
                Times => round(a * b),
                DividedBy => round(a / b),
                EqualTo => Ok(Value::Truth(a == b)),
                NotEqualTo => Ok(Value::Truth(a != b)),
                LessThan => Ok(Value::Truth(a < b)),
                GreaterThan => Ok(Value::Truth(a > b)),
                AtLeast => Ok(Value::Truth(a >= b)),
                AtMost => Ok(Value::Truth(a <= b)),
                RemainderOf | And | Or | JoinedWith => unreachable!(),
            }
        }

        Type::Truth => {
            let (Value::Truth(a), Value::Truth(b)) = (lhs, rhs) else { unreachable!() };
            let (a, b) = (*a, *b);
            match op {
                And => Ok(Value::Truth(a && b)),
                Or => Ok(Value::Truth(a || b)),
                EqualTo => Ok(Value::Truth(a == b)),
                NotEqualTo => Ok(Value::Truth(a != b)),
                _ => unreachable!(),
            }
        }

        Type::Character => {
            let (Value::Char(a), Value::Char(b)) = (lhs, rhs) else { unreachable!() };
            let (a, b) = (*a as u32, *b as u32);
            match op {
                EqualTo => Ok(Value::Truth(a == b)),
                NotEqualTo => Ok(Value::Truth(a != b)),
                LessThan => Ok(Value::Truth(a < b)),
                GreaterThan => Ok(Value::Truth(a > b)),
                AtLeast => Ok(Value::Truth(a >= b)),
                AtMost => Ok(Value::Truth(a <= b)),
                _ => unreachable!(),
            }
        }

        Type::Text => {
            let (Value::Text(a), Value::Text(b)) = (lhs, rhs) else { unreachable!() };
            match op {
                JoinedWith => Ok(Value::Text(format!("{}{}", a, b))),
                EqualTo => Ok(Value::Truth(a == b)),
                NotEqualTo => Ok(Value::Truth(a != b)),
                _ => unreachable!(),
            }
        }
    }
}

fn check_divisor(
    a: i128,
    b: i128,
    width: u32,
    signed: bool,
    zero_fault: Fault,
) -> Result<(), RuntimeError> {
    if b == 0 {
        return Err(RuntimeError { fault: zero_fault });
    }
    if signed && b == -1 && a == min_int(width, signed) {
        return Err(RuntimeError { fault: Fault::DivideOverflow });
    }
    Ok(())
}

impl Value {
    pub fn truth(&self) -> bool {
        match self {
            Value::Truth(b) => *b,
            _ => unreachable!("the checker guarantees conditions are truths"),
        }
    }

    pub fn default_for(ty: Type) -> Value {
        match ty {
            Type::Int { .. } => Value::Int(0),
            Type::Float(_) => Value::Float(0.0),
            Type::Truth => Value::Truth(false),
            Type::Character => Value::Char('\0'),
            Type::Text => Value::Text(String::new()),
        }
    }
}

/// Round a literal double into the layout it is being stored in.
pub fn round_float(kind: FloatKind, v: f64) -> f64 {
    kind.round(v)
}
