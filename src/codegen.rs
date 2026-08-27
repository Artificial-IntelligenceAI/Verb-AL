//! The native backend. Everything here exists to reproduce, in LLVM IR,
//! exactly what `interp.rs` does — including how it formats output and how it
//! fails.

use std::collections::HashMap;
use std::num::NonZeroU32;
use std::path::Path;

use inkwell::basic_block::BasicBlock;
use inkwell::builder::Builder;
use inkwell::context::Context;
use inkwell::module::{Linkage, Module};
use inkwell::targets::{CodeModel, FileType, InitializationConfig, RelocMode, Target, TargetMachine};
use inkwell::types::{BasicTypeEnum, StructType};
use inkwell::values::{
    BasicMetadataValueEnum, BasicValue, BasicValueEnum, FloatValue, FunctionValue, IntValue,
    PointerValue,
};
use inkwell::{AddressSpace, FloatPredicate, IntPredicate, OptimizationLevel};

use crate::ast::{BinOp, Privacy, UnOp};
use crate::tast::*;
use crate::types::{FloatKind, Type};
use crate::value::{min_int, Fault, Value, FAULT_EXIT};

pub struct Backend<'ctx> {
    ctx: &'ctx Context,
    pub module: Module<'ctx>,
    builder: Builder<'ctx>,
    text_ty: StructType<'ctx>,
    statics: Vec<PointerValue<'ctx>>,
    autos: Vec<PointerValue<'ctx>>,
    main: FunctionValue<'ctx>,
    printf: FunctionValue<'ctx>,
    malloc: FunctionValue<'ctx>,
    memcpy: FunctionValue<'ctx>,
    memcmp: FunctionValue<'ctx>,
    write: FunctionValue<'ctx>,
    exit: FunctionValue<'ctx>,
    say_character: Option<FunctionValue<'ctx>>,
    faults: HashMap<Fault, BasicBlock<'ctx>>,
    strings: HashMap<String, PointerValue<'ctx>>,
    anon: usize,
}

pub fn compile<'ctx>(ctx: &'ctx Context, program: &Program, name: &str) -> Module<'ctx> {
    let module = ctx.create_module(name);
    let builder = ctx.create_builder();
    let ptr = ctx.ptr_type(AddressSpace::default());
    let i8t = ctx.i8_type();
    let i32t = ctx.i32_type();
    let i64t = ctx.i64_type();

    // Text is a pointer and a length, exactly as its descriptor says.
    let text_ty = ctx.struct_type(&[ptr.into(), i64t.into()], false);

    let printf = module.add_function(
        "printf",
        i32t.fn_type(&[ptr.into()], true),
        Some(Linkage::External),
    );
    let malloc = module.add_function(
        "malloc",
        ptr.fn_type(&[i64t.into()], false),
        Some(Linkage::External),
    );
    let memcpy = module.add_function(
        "memcpy",
        ptr.fn_type(&[ptr.into(), ptr.into(), i64t.into()], false),
        Some(Linkage::External),
    );
    let memcmp = module.add_function(
        "memcmp",
        i32t.fn_type(&[ptr.into(), ptr.into(), i64t.into()], false),
        Some(Linkage::External),
    );
    let write = module.add_function(
        "write",
        i64t.fn_type(&[i32t.into(), ptr.into(), i64t.into()], false),
        Some(Linkage::External),
    );
    let exit = module.add_function(
        "exit",
        ctx.void_type().fn_type(&[i32t.into()], false),
        Some(Linkage::External),
    );
    let _ = i8t;

    let main = module.add_function("main", i32t.fn_type(&[], false), None);

    let mut backend = Backend {
        ctx,
        module,
        builder,
        text_ty,
        statics: Vec::new(),
        autos: Vec::new(),
        main,
        printf,
        malloc,
        memcpy,
        memcmp,
        write,
        exit,
        say_character: None,
        faults: HashMap::new(),
        strings: HashMap::new(),
        anon: 0,
    };
    backend.emit(program);
    backend.module
}

impl<'ctx> Backend<'ctx> {
    fn emit(&mut self, program: &Program) {
        // Static cells: real globals, filled with the folded initializer.
        for (i, s) in program.statics.iter().enumerate() {
            let ty = self.llvm_ty(s.ty);
            let global = self.module.add_global(ty, None, &symbol_for(i, &s.name));
            global.set_initializer(&self.const_value(&s.init, s.ty));
            global.set_linkage(match s.privacy {
                Privacy::Local => Linkage::Internal,
                Privacy::Public => Linkage::External,
            });
            self.statics.push(global.as_pointer_value());
        }

        let entry = self.ctx.append_basic_block(self.main, "entry");
        self.builder.position_at_end(entry);

        // Automatic cells: stack slots, zeroed so an unreached declaration
        // never leaves rubbish behind.
        for a in &program.autos {
            let ty = self.llvm_ty(a.ty);
            let slot = self.builder.build_alloca(ty, "auto").unwrap();
            self.builder.build_store(slot, zeroed(ty)).unwrap();
            self.autos.push(slot);
        }

        self.block(&program.body);
        self.builder
            .build_return(Some(&self.ctx.i32_type().const_zero()))
            .unwrap();
    }

    // ---- types and constants ---------------------------------------------

    /// Verb-AL widths are already validated as 1..=64, so this cannot fail.
    fn int_ty(&self, width: u32) -> inkwell::types::IntType<'ctx> {
        self.ctx
            .custom_width_int_type(NonZeroU32::new(width).expect("a width is never zero"))
            .expect("a width of 1 to 64 bits is always available")
    }

    fn llvm_ty(&self, ty: Type) -> BasicTypeEnum<'ctx> {
        match ty {
            Type::Int { width, .. } => self.int_ty(width).into(),
            Type::Float(FloatKind::Half) => self.ctx.f16_type().into(),
            Type::Float(FloatKind::BFloat) => self.ctx.bf16_type().into(),
            Type::Float(FloatKind::Single) => self.ctx.f32_type().into(),
            Type::Float(FloatKind::Double) => self.ctx.f64_type().into(),
            Type::Truth => self.ctx.bool_type().into(),
            Type::Character => self.ctx.i32_type().into(),
            Type::Text => self.text_ty.into(),
        }
    }

    fn const_value(&mut self, v: &Value, ty: Type) -> BasicValueEnum<'ctx> {
        match (v, ty) {
            (Value::Int(n), Type::Int { width, .. }) => {
                self.int_ty(width).const_int(*n as i64 as u64, true).into()
            }
            (Value::Float(f), Type::Float(_)) => {
                self.llvm_ty(ty).into_float_type().const_float(*f).into()
            }
            (Value::Truth(b), _) => self.ctx.bool_type().const_int(*b as u64, false).into(),
            (Value::Char(c), _) => self.ctx.i32_type().const_int(*c as u32 as u64, false).into(),
            (Value::Text(s), _) => self.const_text(s).into(),
            _ => unreachable!("checked programs never mix a value with the wrong type"),
        }
    }

    /// A text constant: a private byte array, plus the pointer-and-length pair
    /// that names it.
    fn const_text(&mut self, s: &str) -> inkwell::values::StructValue<'ctx> {
        let bytes = self.ctx.const_string(s.as_bytes(), false);
        self.anon += 1;
        let global = self.module.add_global(bytes.get_type(), None, &format!("verbal.text.{}", self.anon));
        global.set_initializer(&bytes);
        global.set_constant(true);
        global.set_linkage(Linkage::Private);
        let len = self.ctx.i64_type().const_int(s.len() as u64, false);
        self.text_ty.const_named_struct(&[global.as_pointer_value().into(), len.into()])
    }

    /// A NUL-terminated C string, shared between every use of the same text.
    fn cstring(&mut self, s: &str) -> PointerValue<'ctx> {
        if let Some(p) = self.strings.get(s) {
            return *p;
        }
        let g = self.builder.build_global_string_ptr(s, "verbal.cstr").unwrap();
        let p = g.as_pointer_value();
        self.strings.insert(s.to_string(), p);
        p
    }

    // ---- statements -------------------------------------------------------

    fn block(&mut self, stmts: &[TStmt]) {
        for stmt in stmts {
            self.statement(stmt);
        }
    }

    fn statement(&mut self, stmt: &TStmt) {
        match stmt {
            TStmt::InitAuto { slot, init, .. } => {
                let v = self.eval(init);
                self.builder.build_store(self.autos[*slot], v).unwrap();
            }
            TStmt::Assign { place, value, .. } => {
                let v = self.eval(value);
                self.builder.build_store(self.place_ptr(*place), v).unwrap();
            }
            TStmt::Print { parts } => {
                for part in parts {
                    let ty = part.ty();
                    let v = self.eval(part);
                    self.say(v, ty);
                }
                let newline = self.cstring("\n");
                self.call_printf(newline, &[]);
            }
            TStmt::Branch { cond, then, otherwise } => {
                let c = self.eval(cond).into_int_value();
                let then_bb = self.ctx.append_basic_block(self.main, "then");
                let else_bb = self.ctx.append_basic_block(self.main, "otherwise");
                let join_bb = self.ctx.append_basic_block(self.main, "after.branch");
                self.builder.build_conditional_branch(c, then_bb, else_bb).unwrap();

                self.builder.position_at_end(then_bb);
                self.block(then);
                self.builder.build_unconditional_branch(join_bb).unwrap();

                self.builder.position_at_end(else_bb);
                self.block(otherwise);
                self.builder.build_unconditional_branch(join_bb).unwrap();

                self.builder.position_at_end(join_bb);
            }
            TStmt::Repeat { cond, body } => {
                let test_bb = self.ctx.append_basic_block(self.main, "while");
                let body_bb = self.ctx.append_basic_block(self.main, "do");
                let done_bb = self.ctx.append_basic_block(self.main, "after.repetition");
                self.builder.build_unconditional_branch(test_bb).unwrap();

                self.builder.position_at_end(test_bb);
                let c = self.eval(cond).into_int_value();
                self.builder.build_conditional_branch(c, body_bb, done_bb).unwrap();

                self.builder.position_at_end(body_bb);
                self.block(body);
                self.builder.build_unconditional_branch(test_bb).unwrap();

                self.builder.position_at_end(done_bb);
            }
        }
    }

    fn place_ptr(&self, place: Place) -> PointerValue<'ctx> {
        match place {
            Place::Static(i) => self.statics[i],
            Place::Auto(i) => self.autos[i],
        }
    }

    // ---- expressions ------------------------------------------------------

    fn eval(&mut self, e: &TExpr) -> BasicValueEnum<'ctx> {
        match e {
            TExpr::Const { value, ty } => self.const_value(value, *ty),
            TExpr::Read { place, ty } => {
                let pointee = self.llvm_ty(*ty);
                self.builder.build_load(pointee, self.place_ptr(*place), "read").unwrap()
            }
            TExpr::Unary { op, ty, operand } => {
                let v = self.eval(operand);
                match op {
                    UnOp::Not => self.builder.build_not(v.into_int_value(), "not").unwrap().into(),
                    UnOp::Negated => match ty {
                        Type::Int { .. } => {
                            self.builder.build_int_neg(v.into_int_value(), "negated").unwrap().into()
                        }
                        _ => self
                            .builder
                            .build_float_neg(v.into_float_value(), "negated")
                            .unwrap()
                            .into(),
                    },
                }
            }
            TExpr::Binary { op, operand_ty, lhs, rhs, .. } => {
                let a = self.eval(lhs);
                let b = self.eval(rhs);
                self.binary(*op, *operand_ty, a, b)
            }
        }
    }

    fn binary(
        &mut self,
        op: BinOp,
        ty: Type,
        a: BasicValueEnum<'ctx>,
        b: BasicValueEnum<'ctx>,
    ) -> BasicValueEnum<'ctx> {
        use BinOp::*;
        match ty {
            Type::Int { width, signed } => {
                let (a, b) = (a.into_int_value(), b.into_int_value());
                let bd = &self.builder;
                match op {
                    // No nsw/nuw: Verb-AL integers wrap, and so must these.
                    Plus => bd.build_int_add(a, b, "plus").unwrap().into(),
                    Minus => bd.build_int_sub(a, b, "minus").unwrap().into(),
                    Times => bd.build_int_mul(a, b, "times").unwrap().into(),
                    DividedBy | RemainderOf => {
                        let zero_fault =
                            if op == DividedBy { Fault::DivideByZero } else { Fault::RemainderByZero };
                        self.guard_divisor(a, b, width, signed, zero_fault);
                        let bd = &self.builder;
                        match (op, signed) {
                            (DividedBy, true) => bd.build_int_signed_div(a, b, "div").unwrap(),
                            (DividedBy, false) => bd.build_int_unsigned_div(a, b, "div").unwrap(),
                            (_, true) => bd.build_int_signed_rem(a, b, "rem").unwrap(),
                            (_, false) => bd.build_int_unsigned_rem(a, b, "rem").unwrap(),
                        }
                        .into()
                    }
                    _ => {
                        let pred = int_predicate(op, signed);
                        bd.build_int_compare(pred, a, b, op.word()).unwrap().into()
                    }
                }
            }

            Type::Float(_) => {
                let (a, b) = (a.into_float_value(), b.into_float_value());
                let bd = &self.builder;
                match op {
                    Plus => bd.build_float_add(a, b, "plus").unwrap().into(),
                    Minus => bd.build_float_sub(a, b, "minus").unwrap().into(),
                    Times => bd.build_float_mul(a, b, "times").unwrap().into(),
                    DividedBy => bd.build_float_div(a, b, "div").unwrap().into(),
                    _ => bd
                        .build_float_compare(float_predicate(op), a, b, op.word())
                        .unwrap()
                        .into(),
                }
            }

            Type::Truth => {
                let (a, b) = (a.into_int_value(), b.into_int_value());
                let bd = &self.builder;
                match op {
                    And => bd.build_and(a, b, "and").unwrap().into(),
                    Or => bd.build_or(a, b, "or").unwrap().into(),
                    EqualTo => bd.build_int_compare(IntPredicate::EQ, a, b, "eq").unwrap().into(),
                    _ => bd.build_int_compare(IntPredicate::NE, a, b, "ne").unwrap().into(),
                }
            }

            Type::Character => {
                let (a, b) = (a.into_int_value(), b.into_int_value());
                let pred = int_predicate(op, false);
                self.builder.build_int_compare(pred, a, b, op.word()).unwrap().into()
            }

            Type::Text => match op {
                JoinedWith => self.text_join(a, b),
                EqualTo => self.text_equal(a, b, false),
                _ => self.text_equal(a, b, true),
            },
        }
    }

    /// `divided-by` and `remainder-of` check their divisor first, so the
    /// compiled program faults where the interpreter faults instead of
    /// wandering into undefined behaviour.
    fn guard_divisor(
        &mut self,
        a: IntValue<'ctx>,
        b: IntValue<'ctx>,
        width: u32,
        signed: bool,
        zero_fault: Fault,
    ) {
        let int_ty = self.int_ty(width);
        let is_zero = self
            .builder
            .build_int_compare(IntPredicate::EQ, b, int_ty.const_zero(), "divisor.zero")
            .unwrap();
        let ok = self.ctx.append_basic_block(self.main, "divisor.ok");
        let fault = self.fault_block(zero_fault);
        self.builder.build_conditional_branch(is_zero, fault, ok).unwrap();
        self.builder.position_at_end(ok);

        if !signed {
            return;
        }
        // The one signed division that has no representable answer.
        let minus_one = int_ty.const_all_ones();
        let least = int_ty.const_int(min_int(width, true) as i64 as u64, true);
        let b_is_minus_one = self
            .builder
            .build_int_compare(IntPredicate::EQ, b, minus_one, "divisor.minus.one")
            .unwrap();
        let a_is_least =
            self.builder.build_int_compare(IntPredicate::EQ, a, least, "dividend.least").unwrap();
        let both = self.builder.build_and(b_is_minus_one, a_is_least, "overflows").unwrap();
        let ok2 = self.ctx.append_basic_block(self.main, "divisor.fits");
        let fault = self.fault_block(Fault::DivideOverflow);
        self.builder.build_conditional_branch(both, fault, ok2).unwrap();
        self.builder.position_at_end(ok2);
    }

    /// One block per kind of fault, shared by every site that can raise it.
    fn fault_block(&mut self, fault: Fault) -> BasicBlock<'ctx> {
        if let Some(bb) = self.faults.get(&fault) {
            return *bb;
        }
        let here = self.builder.get_insert_block().expect("building inside a block");
        let bb = self.ctx.append_basic_block(self.main, "fault");
        self.builder.position_at_end(bb);

        let msg = fault.message();
        let text = self.const_text(msg);
        let ptr = self.builder.build_extract_value(text, 0, "msg.ptr").unwrap();
        let len = self.builder.build_extract_value(text, 1, "msg.len").unwrap();
        let stderr_fd = self.ctx.i32_type().const_int(2, false);
        self.builder
            .build_call(self.write, &[stderr_fd.into(), ptr.into(), len.into()], "")
            .unwrap();
        let status = self.ctx.i32_type().const_int(FAULT_EXIT as u64, false);
        self.builder.build_call(self.exit, &[status.into()], "").unwrap();
        self.builder.build_unreachable().unwrap();

        self.builder.position_at_end(here);
        self.faults.insert(fault, bb);
        bb
    }

    // ---- text -------------------------------------------------------------

    fn text_parts(&self, v: BasicValueEnum<'ctx>) -> (PointerValue<'ctx>, IntValue<'ctx>) {
        let s = v.into_struct_value();
        let ptr = self.builder.build_extract_value(s, 0, "text.ptr").unwrap().into_pointer_value();
        let len = self.builder.build_extract_value(s, 1, "text.len").unwrap().into_int_value();
        (ptr, len)
    }

    fn text_value(&self, ptr: PointerValue<'ctx>, len: IntValue<'ctx>) -> BasicValueEnum<'ctx> {
        let undef = self.text_ty.get_undef();
        let with_ptr = self.builder.build_insert_value(undef, ptr, 0, "text.with.ptr").unwrap();
        self.builder.build_insert_value(with_ptr, len, 1, "text").unwrap().as_basic_value_enum()
    }

    fn text_join(&mut self, a: BasicValueEnum<'ctx>, b: BasicValueEnum<'ctx>) -> BasicValueEnum<'ctx> {
        let (ap, al) = self.text_parts(a);
        let (bp, bl) = self.text_parts(b);
        let total = self.builder.build_int_add(al, bl, "joined.len").unwrap();
        // One spare byte, so an empty join still asks for a real allocation.
        let ask = self
            .builder
            .build_int_add(total, self.ctx.i64_type().const_int(1, false), "joined.ask")
            .unwrap();
        let buf = self
            .builder
            .build_call(self.malloc, &[ask.into()], "joined.buf")
            .unwrap()
            .try_as_basic_value()
            .expect_basic("malloc returns a pointer")
            .into_pointer_value();
        self.builder.build_call(self.memcpy, &[buf.into(), ap.into(), al.into()], "").unwrap();
        let tail = unsafe {
            self.builder.build_gep(self.ctx.i8_type(), buf, &[al], "joined.tail").unwrap()
        };
        self.builder.build_call(self.memcpy, &[tail.into(), bp.into(), bl.into()], "").unwrap();
        self.text_value(buf, total)
    }

    /// Equal texts have equal lengths and equal bytes; `negate` flips the
    /// answer for `not-equal-to`.
    fn text_equal(
        &mut self,
        a: BasicValueEnum<'ctx>,
        b: BasicValueEnum<'ctx>,
        negate: bool,
    ) -> BasicValueEnum<'ctx> {
        let (ap, al) = self.text_parts(a);
        let (bp, bl) = self.text_parts(b);
        let same_len =
            self.builder.build_int_compare(IntPredicate::EQ, al, bl, "text.same.len").unwrap();
        let entry = self.builder.get_insert_block().unwrap();
        let cmp_bb = self.ctx.append_basic_block(self.main, "text.compare");
        let join_bb = self.ctx.append_basic_block(self.main, "text.compared");
        self.builder.build_conditional_branch(same_len, cmp_bb, join_bb).unwrap();

        self.builder.position_at_end(cmp_bb);
        let r = self
            .builder
            .build_call(self.memcmp, &[ap.into(), bp.into(), al.into()], "text.memcmp")
            .unwrap()
            .try_as_basic_value()
            .expect_basic("memcmp returns an int")
            .into_int_value();
        let bytes_equal = self
            .builder
            .build_int_compare(IntPredicate::EQ, r, self.ctx.i32_type().const_zero(), "text.eq")
            .unwrap();
        self.builder.build_unconditional_branch(join_bb).unwrap();

        self.builder.position_at_end(join_bb);
        let phi = self.builder.build_phi(self.ctx.bool_type(), "text.equal").unwrap();
        phi.add_incoming(&[(&self.ctx.bool_type().const_zero(), entry), (&bytes_equal, cmp_bb)]);
        let equal = phi.as_basic_value().into_int_value();
        if negate {
            self.builder.build_not(equal, "text.not.equal").unwrap().into()
        } else {
            equal.into()
        }
    }

    // ---- output -----------------------------------------------------------

    fn say(&mut self, v: BasicValueEnum<'ctx>, ty: Type) {
        match ty {
            Type::Int { width, signed } => {
                let i64t = self.ctx.i64_type();
                let v = v.into_int_value();
                let widened = if width == 64 {
                    v
                } else if signed {
                    self.builder.build_int_s_extend(v, i64t, "say.sext").unwrap()
                } else {
                    self.builder.build_int_z_extend(v, i64t, "say.zext").unwrap()
                };
                let fmt = if signed { "%lld" } else { "%llu" };
                let fmt = self.cstring(fmt);
                self.call_printf(fmt, &[widened.into()]);
            }

            Type::Float(kind) => {
                let f = v.into_float_value();
                let widened: FloatValue<'ctx> = if kind == FloatKind::Double {
                    f
                } else {
                    self.builder.build_float_ext(f, self.ctx.f64_type(), "say.fpext").unwrap()
                };
                let fmt = self.cstring(&format!("%.{}g", kind.print_precision()));
                self.call_printf(fmt, &[widened.into()]);
            }

            Type::Truth => {
                let yes = self.cstring("true");
                let no = self.cstring("false");
                let chosen = self
                    .builder
                    .build_select(v.into_int_value(), yes, no, "say.truth")
                    .unwrap()
                    .into_pointer_value();
                let fmt = self.cstring("%s");
                self.call_printf(fmt, &[chosen.into()]);
            }

            Type::Character => {
                let f = self.get_say_character();
                self.builder.build_call(f, &[v.into()], "").unwrap();
            }

            Type::Text => {
                let (ptr, len) = self.text_parts(v);
                let len32 =
                    self.builder.build_int_truncate(len, self.ctx.i32_type(), "say.len").unwrap();
                let fmt = self.cstring("%.*s");
                self.call_printf(fmt, &[len32.into(), ptr.into()]);
            }
        }
    }

    fn call_printf(&mut self, fmt: PointerValue<'ctx>, args: &[BasicMetadataValueEnum<'ctx>]) {
        let mut all: Vec<BasicMetadataValueEnum<'ctx>> = vec![fmt.into()];
        all.extend_from_slice(args);
        self.builder.build_call(self.printf, &all, "").unwrap();
    }

    /// Writes one Unicode scalar as UTF-8, the same bytes Rust's `encode_utf8`
    /// produces for the interpreter.
    fn get_say_character(&mut self) -> FunctionValue<'ctx> {
        if let Some(f) = self.say_character {
            return f;
        }
        let here = self.builder.get_insert_block().expect("building inside a block");
        let i32t = self.ctx.i32_type();
        let i8t = self.ctx.i8_type();
        let f = self.module.add_function(
            "verbal.say_character",
            self.ctx.void_type().fn_type(&[i32t.into()], false),
            Some(Linkage::Internal),
        );
        let entry = self.ctx.append_basic_block(f, "entry");
        self.builder.position_at_end(entry);

        let c = f.get_nth_param(0).unwrap().into_int_value();
        let buf = self.builder.build_array_alloca(i8t, i32t.const_int(5, false), "buf").unwrap();
        let n = self.builder.build_alloca(i32t, "n").unwrap();

        let one_bb = self.ctx.append_basic_block(f, "one.byte");
        let try_two = self.ctx.append_basic_block(f, "try.two");
        let two_bb = self.ctx.append_basic_block(f, "two.bytes");
        let try_three = self.ctx.append_basic_block(f, "try.three");
        let three_bb = self.ctx.append_basic_block(f, "three.bytes");
        let four_bb = self.ctx.append_basic_block(f, "four.bytes");
        let done = self.ctx.append_basic_block(f, "done");

        let put = |be: &Backend<'ctx>, index: u64, value: IntValue<'ctx>| {
            let slot = unsafe {
                be.builder
                    .build_gep(i8t, buf, &[i32t.const_int(index, false)], "slot")
                    .unwrap()
            };
            be.builder.build_store(slot, value).unwrap();
        };
        let byte = |be: &Backend<'ctx>, v: IntValue<'ctx>| {
            be.builder.build_int_truncate(v, i8t, "byte").unwrap()
        };
        let shift = |be: &Backend<'ctx>, v: IntValue<'ctx>, by: u64| {
            be.builder.build_right_shift(v, i32t.const_int(by, false), false, "shifted").unwrap()
        };
        let mask = |be: &Backend<'ctx>, v: IntValue<'ctx>| {
            be.builder.build_and(v, i32t.const_int(0x3F, false), "low.six").unwrap()
        };
        let tag = |be: &Backend<'ctx>, v: IntValue<'ctx>, t: u64| {
            be.builder.build_or(v, i32t.const_int(t, false), "tagged").unwrap()
        };

        let below = |be: &Backend<'ctx>, limit: u64| {
            be.builder
                .build_int_compare(IntPredicate::ULT, c, i32t.const_int(limit, false), "below")
                .unwrap()
        };

        let cond = below(self, 0x80);
        self.builder.build_conditional_branch(cond, one_bb, try_two).unwrap();

        self.builder.position_at_end(one_bb);
        let b0 = byte(self, c);
        put(self, 0, b0);
        self.builder.build_store(n, i32t.const_int(1, false)).unwrap();
        self.builder.build_unconditional_branch(done).unwrap();

        self.builder.position_at_end(try_two);
        let cond = below(self, 0x800);
        self.builder.build_conditional_branch(cond, two_bb, try_three).unwrap();

        self.builder.position_at_end(two_bb);
        let hi = tag(self, shift(self, c, 6), 0xC0);
        let lo = tag(self, mask(self, c), 0x80);
        let (hi, lo) = (byte(self, hi), byte(self, lo));
        put(self, 0, hi);
        put(self, 1, lo);
        self.builder.build_store(n, i32t.const_int(2, false)).unwrap();
        self.builder.build_unconditional_branch(done).unwrap();

        self.builder.position_at_end(try_three);
        let cond = below(self, 0x10000);
        self.builder.build_conditional_branch(cond, three_bb, four_bb).unwrap();

        self.builder.position_at_end(three_bb);
        let a = tag(self, shift(self, c, 12), 0xE0);
        let m = tag(self, mask(self, shift(self, c, 6)), 0x80);
        let l = tag(self, mask(self, c), 0x80);
        let (a, m, l) = (byte(self, a), byte(self, m), byte(self, l));
        put(self, 0, a);
        put(self, 1, m);
        put(self, 2, l);
        self.builder.build_store(n, i32t.const_int(3, false)).unwrap();
        self.builder.build_unconditional_branch(done).unwrap();

        self.builder.position_at_end(four_bb);
        let a = tag(self, shift(self, c, 18), 0xF0);
        let b = tag(self, mask(self, shift(self, c, 12)), 0x80);
        let m = tag(self, mask(self, shift(self, c, 6)), 0x80);
        let l = tag(self, mask(self, c), 0x80);
        let (a, b, m, l) = (byte(self, a), byte(self, b), byte(self, m), byte(self, l));
        put(self, 0, a);
        put(self, 1, b);
        put(self, 2, m);
        put(self, 3, l);
        self.builder.build_store(n, i32t.const_int(4, false)).unwrap();
        self.builder.build_unconditional_branch(done).unwrap();

        self.builder.position_at_end(done);
        let count = self.builder.build_load(i32t, n, "count").unwrap().into_int_value();
        let terminator =
            unsafe { self.builder.build_gep(i8t, buf, &[count], "terminator").unwrap() };
        self.builder.build_store(terminator, i8t.const_zero()).unwrap();
        let fmt = self.cstring("%s");
        self.builder.build_call(self.printf, &[fmt.into(), buf.into()], "").unwrap();
        self.builder.build_return(None).unwrap();

        self.builder.position_at_end(here);
        self.say_character = Some(f);
        f
    }
}

fn int_predicate(op: BinOp, signed: bool) -> IntPredicate {
    use BinOp::*;
    match (op, signed) {
        (EqualTo, _) => IntPredicate::EQ,
        (NotEqualTo, _) => IntPredicate::NE,
        (LessThan, true) => IntPredicate::SLT,
        (LessThan, false) => IntPredicate::ULT,
        (GreaterThan, true) => IntPredicate::SGT,
        (GreaterThan, false) => IntPredicate::UGT,
        (AtLeast, true) => IntPredicate::SGE,
        (AtLeast, false) => IntPredicate::UGE,
        (AtMost, true) => IntPredicate::SLE,
        (AtMost, false) => IntPredicate::ULE,
        _ => unreachable!(),
    }
}

/// `not-equal-to` is unordered so that NaN differs from everything, matching
/// the interpreter's `!=`; every other comparison is ordered.
fn float_predicate(op: BinOp) -> FloatPredicate {
    use BinOp::*;
    match op {
        EqualTo => FloatPredicate::OEQ,
        NotEqualTo => FloatPredicate::UNE,
        LessThan => FloatPredicate::OLT,
        GreaterThan => FloatPredicate::OGT,
        AtLeast => FloatPredicate::OGE,
        AtMost => FloatPredicate::OLE,
        _ => unreachable!(),
    }
}

fn zeroed(ty: BasicTypeEnum<'_>) -> BasicValueEnum<'_> {
    match ty {
        BasicTypeEnum::IntType(t) => t.const_zero().into(),
        BasicTypeEnum::FloatType(t) => t.const_zero().into(),
        BasicTypeEnum::StructType(t) => t.const_zero().into(),
        BasicTypeEnum::PointerType(t) => t.const_zero().into(),
        BasicTypeEnum::ArrayType(t) => t.const_zero().into(),
        BasicTypeEnum::VectorType(t) => t.const_zero().into(),
        BasicTypeEnum::ScalableVectorType(t) => t.const_zero().into(),
    }
}

/// Verb-AL names may hold spaces and emoji; object-file symbols may not, so a
/// declaration's symbol pairs its index with an ASCII echo of its name.
fn symbol_for(index: usize, name: &str) -> String {
    let echo: String = name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    format!("verbal.{}.{}", index, echo)
}

// ---- emitting artefacts ---------------------------------------------------

pub fn write_object(module: &Module<'_>, path: &Path) -> Result<(), String> {
    Target::initialize_all(&InitializationConfig::default());
    let triple = TargetMachine::get_default_triple();
    let target = Target::from_triple(&triple).map_err(|e| e.to_string())?;
    let machine = target
        .create_target_machine(
            &triple,
            TargetMachine::get_host_cpu_name().to_str().unwrap_or("generic"),
            TargetMachine::get_host_cpu_features().to_str().unwrap_or(""),
            OptimizationLevel::Default,
            RelocMode::PIC,
            CodeModel::Default,
        )
        .ok_or("this host has no target machine")?;
    module.set_triple(&triple);
    machine
        .write_to_file(module, FileType::Object, path)
        .map_err(|e| e.to_string())
}

pub fn jit(module: &Module<'_>) -> Result<i32, String> {
    let engine = module
        .create_jit_execution_engine(OptimizationLevel::None)
        .map_err(|e| e.to_string())?;
    unsafe {
        let main: inkwell::execution_engine::JitFunction<unsafe extern "C" fn() -> i32> =
            engine.get_function("main").map_err(|e| e.to_string())?;
        Ok(main.call())
    }
}
