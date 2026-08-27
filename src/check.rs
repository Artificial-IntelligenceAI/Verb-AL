//! Checking turns the parse tree into a fully resolved program: types settled,
//! literals converted, names replaced by slot indices, static initializers
//! folded. Everything downstream is then mechanical.

use std::collections::HashMap;

use crate::ast::{self, BinOp, Expr, MemoryClass, Stmt, UnOp};
use crate::diag::{Diag, Span};
use crate::tast::*;
use crate::types::{classes_of, describe_classes, Type};
use crate::value::{self, Value};

pub fn check(stmts: &[Stmt]) -> Result<Program, Diag> {
    let mut c = Checker { scopes: vec![HashMap::new()], statics: Vec::new(), autos: Vec::new() };
    let body = c.statements(stmts)?;
    Ok(Program { statics: c.statics, autos: c.autos, body })
}

#[derive(Clone, Copy)]
struct Binding {
    place: Place,
    ty: Type,
}

struct Checker {
    scopes: Vec<HashMap<String, Binding>>,
    statics: Vec<StaticVar>,
    autos: Vec<AutoVar>,
}

impl Checker {
    fn lookup(&self, name: &str) -> Option<Binding> {
        self.scopes.iter().rev().find_map(|s| s.get(name).copied())
    }

    fn block(&mut self, stmts: &[Stmt]) -> Result<Vec<TStmt>, Diag> {
        self.scopes.push(HashMap::new());
        let out = self.statements(stmts);
        self.scopes.pop();
        out
    }

    fn statements(&mut self, stmts: &[Stmt]) -> Result<Vec<TStmt>, Diag> {
        let mut out = Vec::new();
        for stmt in stmts {
            if let Some(t) = self.statement(stmt)? {
                out.push(t);
            }
        }
        Ok(out)
    }

    fn statement(&mut self, stmt: &Stmt) -> Result<Option<TStmt>, Diag> {
        match stmt {
            // A note is addressed to the reader, not the machine.
            Stmt::Note { .. } => Ok(None),

            Stmt::Decl(d) => self.declaration(d),

            Stmt::Say { source, .. } => {
                let value = self.expr(source, None)?;
                let ty = value.ty();
                Ok(Some(TStmt::Say { value, ty }))
            }

            Stmt::Assign { target, target_span, value, .. } => {
                let Some(binding) = self.lookup(target) else {
                    return Err(unknown_name(target, *target_span));
                };
                let value = self.expr(value, Some(binding.ty))?;
                Ok(Some(TStmt::Assign { place: binding.place, ty: binding.ty, value }))
            }

            Stmt::Branch { cond, then, otherwise, .. } => {
                let cond = self.expr(cond, Some(Type::Truth))?;
                let then = self.block(then)?;
                let otherwise = self.block(otherwise)?;
                Ok(Some(TStmt::Branch { cond, then, otherwise }))
            }

            Stmt::Repeat { cond, body, .. } => {
                let cond = self.expr(cond, Some(Type::Truth))?;
                let body = self.block(body)?;
                Ok(Some(TStmt::Repeat { cond, body }))
            }
        }
    }

    fn declaration(&mut self, d: &ast::Decl) -> Result<Option<TStmt>, Diag> {
        self.verify_name_descriptor(d)?;

        if self.lookup(&d.name).is_some() {
            return Err(Diag::new(
                format!("\"{}\" is already declared", d.name),
                d.name_span,
            )
            .note("Verb-AL does not let one name stand for two things"));
        }

        let init = self.expr(&d.init, Some(d.ty))?;

        let (place, stmt) = match d.memory {
            MemoryClass::Static => {
                // A static cell is filled once, before the program begins, so
                // its initializer must be knowable now.
                let folded = fold(&init, d.init.span())?;
                let slot = self.statics.len();
                self.statics.push(StaticVar {
                    name: d.name.clone(),
                    ty: d.ty,
                    privacy: d.privacy,
                    init: folded,
                });
                (Place::Static(slot), None)
            }
            MemoryClass::Automatic => {
                let slot = self.autos.len();
                self.autos.push(AutoVar { name: d.name.clone(), ty: d.ty });
                (Place::Auto(slot), Some(TStmt::InitAuto { slot, ty: d.ty, init }))
            }
        };

        self.scopes
            .last_mut()
            .expect("there is always a scope")
            .insert(d.name.clone(), Binding { place, ty: d.ty });
        Ok(stmt)
    }

    /// The heart of the language: a name may only draw on the classes its
    /// descriptor permits. The descriptor is an allowance, not an inventory —
    /// it may permit more than the name happens to use.
    fn verify_name_descriptor(&self, d: &ast::Decl) -> Result<(), Diag> {
        if d.name.is_empty() {
            return Err(Diag::new("a name cannot be empty", d.name_span));
        }
        let present = classes_of(&d.name).map_err(|c| {
            Diag::new(
                format!("the character `{}` in this name belongs to no class Verb-AL knows", c),
                d.name_span,
            )
            .note("a name may only contain characters some class covers")
        })?;

        let missing: Vec<_> =
            present.iter().copied().filter(|c| !d.name_classes.contains(c)).collect();
        let Some(&first) = missing.first() else { return Ok(()) };

        // Point at a character the reader can actually find in the name.
        let offender = d
            .name
            .chars()
            .find(|c| crate::types::classify(*c) == Some(first))
            .expect("the class was found in this very name");
        let permitted: Vec<_> =
            d.name_classes.iter().copied().chain(missing.iter().copied()).collect();

        Err(Diag::new("this name uses a class its descriptor does not permit", d.name_desc_span)
            .note(format!("the descriptor permits: {}", describe_classes(&d.name_classes)))
            .note(format!(
                "but \"{}\" contains `{}`, which is {}",
                d.name,
                offender,
                first.word()
            ))
            .note(format!("write: name.{}.end", describe_classes(&permitted))))
    }

    // ---- expressions ------------------------------------------------------

    /// What type an expression has without being told. `None` means the
    /// surroundings must decide — which is only ever true of a bare literal.
    fn infer(&self, e: &Expr) -> Option<Type> {
        match e {
            Expr::Ident { name, .. } => self.lookup(name).map(|b| b.ty),
            Expr::Lit { .. } => None,
            Expr::Unary { op: UnOp::Not, .. } => Some(Type::Truth),
            Expr::Unary { op: UnOp::Negated, operand, .. } => self.infer(operand),
            Expr::Binary { op, lhs, rhs, .. } => {
                if op.yields_truth() {
                    Some(Type::Truth)
                } else if *op == BinOp::JoinedWith {
                    Some(Type::Text)
                } else {
                    self.infer(lhs).or_else(|| self.infer(rhs))
                }
            }
        }
    }

    fn expr(&self, e: &Expr, expected: Option<Type>) -> Result<TExpr, Diag> {
        let got = match e {
            Expr::Ident { name, span } => {
                let Some(binding) = self.lookup(name) else {
                    return Err(unknown_name(name, *span));
                };
                TExpr::Read { place: binding.place, ty: binding.ty }
            }

            Expr::Lit { raw, span } => {
                // With nothing to say otherwise, a literal is text.
                let ty = expected.unwrap_or(Type::Text);
                TExpr::Const { value: parse_literal(raw, ty, *span)?, ty }
            }

            Expr::Unary { op: UnOp::Not, operand, span } => {
                let operand = self.expr(operand, Some(Type::Truth))?;
                let _ = span;
                TExpr::Unary { op: UnOp::Not, ty: Type::Truth, operand: Box::new(operand) }
            }

            Expr::Unary { op: UnOp::Negated, operand, span } => {
                let want = expected.or_else(|| self.infer(operand));
                let Some(want) = want else {
                    return Err(Diag::new(
                        "nothing here says what kind of number to negate",
                        *span,
                    )
                    .note("negate a declared name, or negate inside a declaration or assignment that fixes the type"));
                };
                if !want.is_numeric() {
                    return Err(Diag::new(
                        format!("`negated` needs a number, but this is {}", want.describe()),
                        *span,
                    ));
                }
                let operand = self.expr(operand, Some(want))?;
                TExpr::Unary { op: UnOp::Negated, ty: want, operand: Box::new(operand) }
            }

            Expr::Binary { op, lhs, rhs, op_span, span } => {
                let operand_ty = match op {
                    BinOp::And | BinOp::Or => Type::Truth,
                    BinOp::JoinedWith => Type::Text,
                    _ if op.yields_truth() => {
                        // A comparison learns its operand type from its operands.
                        match self.infer(lhs).or_else(|| self.infer(rhs)) {
                            Some(t) => t,
                            None => {
                                return Err(Diag::new(
                                    format!("nothing here says what kind of values `{}` is comparing", op.word()),
                                    *span,
                                )
                                .note("compare against a declared name, so the type is known"))
                            }
                        }
                    }
                    // Arithmetic yields what it consumes, so the expected type
                    // flows straight through to the operands.
                    _ => match expected.or_else(|| self.infer(lhs).or_else(|| self.infer(rhs))) {
                        Some(t) => t,
                        None => {
                            return Err(Diag::new(
                                format!("nothing here says what kind of values `{}` is combining", op.word()),
                                *span,
                            ))
                        }
                    },
                };

                self.check_operator_applies(*op, operand_ty, *op_span)?;

                let lhs = self.expr(lhs, Some(operand_ty))?;
                let rhs = self.expr(rhs, Some(operand_ty))?;
                let ty = if op.yields_truth() { Type::Truth } else { operand_ty };
                TExpr::Binary {
                    op: *op,
                    ty,
                    operand_ty,
                    lhs: Box::new(lhs),
                    rhs: Box::new(rhs),
                    span: *span,
                }
            }
        };

        if let Some(want) = expected {
            if got.ty() != want {
                return Err(Diag::new(
                    format!("expected {}, found {}", want.describe(), got.ty().describe()),
                    e.span(),
                )
                .note("Verb-AL converts nothing on your behalf"));
            }
        }
        Ok(got)
    }

    fn check_operator_applies(&self, op: BinOp, ty: Type, span: Span) -> Result<(), Diag> {
        use BinOp::*;
        let ok = match op {
            Plus | Minus | Times | DividedBy => ty.is_numeric(),
            RemainderOf => matches!(ty, Type::Int { .. }),
            LessThan | GreaterThan | AtLeast | AtMost => ty.is_ordered(),
            EqualTo | NotEqualTo => true,
            And | Or => ty == Type::Truth,
            JoinedWith => ty == Type::Text,
        };
        if ok {
            return Ok(());
        }
        Err(Diag::new(
            format!("`{}` does not apply to {}", op.word(), ty.describe()),
            span,
        ))
    }
}

fn unknown_name(name: &str, span: Span) -> Diag {
    Diag::new(format!("nothing named \"{}\" has been declared", name), span)
        .note("declare it first, with its privacy, memory, type and name descriptor")
}

/// Fold a static initializer to a value, or explain why it cannot be.
fn fold(e: &TExpr, whole: Span) -> Result<Value, Diag> {
    match e {
        TExpr::Const { value, .. } => Ok(value.clone()),
        TExpr::Read { .. } => Err(Diag::new(
            "a static initializer must be knowable before the program begins, \
             so it cannot read a variable",
            whole,
        )
        .note("declare it `memory:automatic` if it must be worked out while running")),
        TExpr::Unary { op, ty, operand } => {
            let operand = fold(operand, whole)?;
            value::unary(*op, *ty, &operand).map_err(|e| Diag::bare(e.fault.message().trim_end().to_string()))
        }
        TExpr::Binary { op, operand_ty, lhs, rhs, span, .. } => {
            let lhs = fold(lhs, whole)?;
            let rhs = fold(rhs, whole)?;
            value::binary(*op, *operand_ty, &lhs, &rhs).map_err(|e| {
                Diag::new(
                    e.fault.message().trim_end().trim_start_matches("verb-al: ").to_string(),
                    *span,
                )
                .note("a static initializer is worked out while the program is still being built")
            })
        }
    }
}

/// Read a literal as the type its surroundings demand.
pub fn parse_literal(raw: &str, ty: Type, span: Span) -> Result<Value, Diag> {
    match ty {
        Type::Int { width, signed } => {
            let n: i128 = raw.parse().map_err(|_| {
                Diag::new(format!("'{}' is not a whole number", raw), span)
                    .note("write digits, optionally preceded by a minus sign")
            })?;
            let (lo, hi) = if signed {
                (-(1i128 << (width - 1)), (1i128 << (width - 1)) - 1)
            } else {
                (0, (1i128 << width) - 1)
            };
            if n < lo || n > hi {
                return Err(Diag::new(
                    format!("{} does not fit in {}", n, ty.describe()),
                    span,
                )
                .note(format!("that type holds {} through {}", lo, hi)));
            }
            Ok(Value::Int(n))
        }
        Type::Float(kind) => {
            let v: f64 = raw.parse().map_err(|_| {
                Diag::new(format!("'{}' is not a number", raw), span)
            })?;
            Ok(Value::Float(kind.round(v)))
        }
        Type::Truth => match raw {
            "true" => Ok(Value::Truth(true)),
            "false" => Ok(Value::Truth(false)),
            _ => Err(Diag::new(format!("'{}' is not a truth value", raw), span)
                .note("a truth value is written 'true' or 'false'")),
        },
        Type::Character => {
            let mut chars = raw.chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) => Ok(Value::Char(c)),
                (None, _) => Err(Diag::new("a character literal holds no character", span)),
                (Some(_), Some(_)) => Err(Diag::new(
                    format!("'{}' holds more than one character", raw),
                    span,
                )
                .note("a character is one Unicode scalar; use text for more")),
            }
        }
        Type::Text => Ok(Value::Text(raw.to_string())),
    }
}
