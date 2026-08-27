//! The tree-walking backend. It is the reference semantics: whatever this does,
//! the compiled program must do too.

use std::io::Write;

use crate::tast::*;
use crate::types::Type;
use crate::value::{self, RuntimeError, Value};

pub struct Interpreter<W: Write> {
    statics: Vec<Value>,
    autos: Vec<Value>,
    out: W,
}

pub fn run<W: Write>(program: &Program, out: W) -> Result<(), RuntimeError> {
    // Static cells are filled once, before the program begins.
    let statics = program.statics.iter().map(|s| s.init.clone()).collect();
    let autos = program.autos.iter().map(|a| Value::default_for(a.ty)).collect();
    let mut interp = Interpreter { statics, autos, out };
    interp.block(&program.body)
}

impl<W: Write> Interpreter<W> {
    fn block(&mut self, stmts: &[TStmt]) -> Result<(), RuntimeError> {
        for stmt in stmts {
            self.statement(stmt)?;
        }
        Ok(())
    }

    fn statement(&mut self, stmt: &TStmt) -> Result<(), RuntimeError> {
        match stmt {
            TStmt::InitAuto { slot, init, .. } => {
                let v = self.eval(init)?;
                self.autos[*slot] = v;
            }
            TStmt::Say { value, ty } => {
                let v = self.eval(value)?;
                let mut bytes = crate::fmt::say_bytes(&v, *ty);
                bytes.push(b'\n');
                self.out.write_all(&bytes).expect("writing to standard output");
            }
            TStmt::Assign { place, value, .. } => {
                let v = self.eval(value)?;
                self.store(*place, v);
            }
            TStmt::Branch { cond, then, otherwise } => {
                if self.eval(cond)?.truth() {
                    self.block(then)?;
                } else {
                    self.block(otherwise)?;
                }
            }
            TStmt::Repeat { cond, body } => {
                while self.eval(cond)?.truth() {
                    self.block(body)?;
                }
            }
        }
        Ok(())
    }

    fn store(&mut self, place: Place, v: Value) {
        match place {
            Place::Static(i) => self.statics[i] = v,
            Place::Auto(i) => self.autos[i] = v,
        }
    }

    fn eval(&mut self, e: &TExpr) -> Result<Value, RuntimeError> {
        Ok(match e {
            TExpr::Const { value, .. } => value.clone(),
            TExpr::Read { place, .. } => match place {
                Place::Static(i) => self.statics[*i].clone(),
                Place::Auto(i) => self.autos[*i].clone(),
            },
            TExpr::Unary { op, ty, operand } => {
                let v = self.eval(operand)?;
                value::unary(*op, *ty, &v)?
            }
            TExpr::Binary { op, operand_ty, lhs, rhs, .. } => {
                let a = self.eval(lhs)?;
                let b = self.eval(rhs)?;
                value::binary(*op, *operand_ty, &a, &b)?
            }
        })
    }
}

/// Only used by tests and diagnostics: the declared type of a slot.
pub fn slot_type(program: &Program, place: Place) -> Type {
    match place {
        Place::Static(i) => program.statics[i].ty,
        Place::Auto(i) => program.autos[i].ty,
    }
}
