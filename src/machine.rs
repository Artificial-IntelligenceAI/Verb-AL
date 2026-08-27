//! The machine a program is built for, described in the terms a program is
//! allowed to require of it.
//!
//! A Verb-AL program states what it needs of its machine (SPEC §6) and the
//! compiler checks the machine against that claim. Only three facts can be
//! required, because only three affect anything the source can say: how wide a
//! pointer is, which end of a number comes first, and how strictly anything
//! must be aligned. A CPU model or a feature string changes none of them.

use inkwell::context::Context;
use inkwell::targets::{
    ByteOrdering, CodeModel, InitializationConfig, RelocMode, Target, TargetMachine,
};
use inkwell::OptimizationLevel;
use inkwell::AddressSpace;

#[derive(Clone, Debug)]
pub struct Machine {
    pub triple: String,
    pub pointer_bits: u32,
    pub little_endian: bool,
    pub max_alignment: u32,
}

impl Machine {
    /// The requirement line this machine would satisfy, so a diagnostic can
    /// hand back something the programmer may paste.
    pub fn describe(&self) -> String {
        format!(
            "requires:target.{}-bit-pointers.{}-endian.{}-byte-maximum-alignment end",
            self.pointer_bits,
            if self.little_endian { "little" } else { "big" },
            self.max_alignment
        )
    }
}

/// Ask LLVM about the machine this compiler is running on.
pub fn host() -> Result<Machine, String> {
    Target::initialize_all(&InitializationConfig::default());
    let triple = TargetMachine::get_default_triple();
    let target = Target::from_triple(&triple).map_err(|e| e.to_string())?;
    let machine = target
        .create_target_machine(
            &triple,
            TargetMachine::get_host_cpu_name().to_str().unwrap_or("generic"),
            TargetMachine::get_host_cpu_features().to_str().unwrap_or(""),
            OptimizationLevel::None,
            RelocMode::PIC,
            CodeModel::Default,
        )
        .ok_or("this machine has no LLVM target")?;
    let data = machine.get_target_data();

    // The strictest alignment any Verb-AL value asks for. Computed rather than
    // assumed, so the claim a program makes is checked against the machine
    // instead of against a table someone wrote down once.
    let ctx = Context::create();
    let text = ctx.struct_type(
        &[ctx.ptr_type(AddressSpace::default()).into(), ctx.i64_type().into()],
        false,
    );
    let max_alignment = [
        data.get_abi_alignment(&ctx.i64_type()),
        data.get_abi_alignment(&ctx.f64_type()),
        data.get_abi_alignment(&ctx.ptr_type(AddressSpace::default())),
        data.get_abi_alignment(&text),
    ]
    .into_iter()
    .max()
    .expect("the list is not empty");

    Ok(Machine {
        triple: triple.as_str().to_string_lossy().to_string(),
        pointer_bits: data.get_pointer_byte_size(None) * 8,
        little_endian: data.get_byte_ordering() == ByteOrdering::LittleEndian,
        max_alignment,
    })
}
