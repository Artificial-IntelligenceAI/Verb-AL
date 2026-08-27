//! The machine a program is built for, described in the terms a program is
//! allowed to require of it.
//!
//! A Verb-AL program states what it needs of its machine (SPEC §6) and the
//! compiler checks the machine against that claim. Only three facts can be
//! required, because only three affect anything the source can say: how wide a
//! pointer is, which end of a number comes first, and how strictly anything
//! must be aligned. A CPU model or a feature string changes none of them.

use crate::ast::{CodeModelChoice, MachineSpec, Optimisation, Relocation};
use crate::diag::Diag;
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

/// A named machine, ready to emit for.
pub struct Built {
    pub properties: Machine,
    pub target: TargetMachine,
    pub triple: String,
}

/// Turn a `.machine` file into a machine, asking LLVM to referee every claim
/// it can. Only the calling convention is checked against a table the compiler
/// carries, because LLVM will not answer for it — see SPEC §6.1.
pub fn from_spec(spec: &MachineSpec) -> Result<Built, Diag> {
    Target::initialize_all(&InitializationConfig::default());

    let Some(target) = Target::from_name(&spec.arch) else {
        let known = registered_targets();
        let mut diag = Diag::new(
            format!(
                "a machine names an architecture this compiler was built with; `{}` is not one",
                spec.arch
            ),
            spec.arch_span,
            format!("write one of: {}", known.join(", ")),
        );
        if let Some(near) = nearest(&spec.arch, &known) {
            // A near miss is the whole answer; the catalogue after it is noise.
            diag.fix = format!("write `{}`", near);
        }
        return Err(diag);
    };
    if !target.has_target_machine() {
        return Err(Diag::new(
            format!(
                "`{}` is an architecture this compiler knows, but it was built without \
                 the ability to emit code for it",
                spec.arch
            ),
            spec.arch_span,
            format!(
                "rebuild the compiler with the {} target, or name one that can emit: {}",
                spec.arch,
                emitting_targets().join(", ")
            ),
        ));
    }

    let expected = expected_convention(&spec.arch, &spec.os).ok_or_else(|| {
        Diag::new(
            format!(
                "this compiler cannot check the calling convention for `{}`, and will not \
                 accept a claim it cannot check",
                spec.arch
            ),
            spec.calling_convention_span,
            "name an architecture whose convention this compiler knows: aarch64, x86-64, \
             riscv64 or wasm32",
        )
    })?;
    if spec.calling_convention != expected {
        return Err(Diag::new(
            format!(
                "a machine's calling convention follows from its architecture and system; \
                 {} on {} uses {}, not `{}`",
                spec.arch, spec.os, expected, spec.calling_convention
            ),
            spec.calling_convention_span,
            format!("write `{}`", expected),
        ));
    }

    // LLVM will happily build a target machine from a triple with a nonsense
    // object format and only fail at link time, so the name is checked here.
    const FORMATS: &[&str] =
        &["elf", "macho", "coff", "wasm", "xcoff", "goff", "spirv", "dxcontainer"];
    if !FORMATS.contains(&spec.object_format.as_str()) {
        let mut diag = Diag::new(
            format!(
                "a machine names its object format as LLVM names it; `{}` is not one of those",
                spec.object_format
            ),
            spec.system_span,
            format!("write one of: {}", FORMATS.join(", ")),
        );
        let known: Vec<String> = FORMATS.iter().map(|f| f.to_string()).collect();
        if let Some(near) = nearest(&spec.object_format, &known) {
            diag.fix = format!("write `{}`", near);
        }
        return Err(diag);
    }

    let triple_text = spec.triple();
    let triple = inkwell::targets::TargetTriple::create(&triple_text);
    let features = if spec.features.is_empty() { String::new() } else { spec.features.join(",") };
    let target_machine = target
        .create_target_machine(
            &triple,
            &spec.cpu,
            &features,
            match spec.optimisation {
                Optimisation::None => OptimizationLevel::None,
                Optimisation::Less => OptimizationLevel::Less,
                Optimisation::Default => OptimizationLevel::Default,
                Optimisation::Aggressive => OptimizationLevel::Aggressive,
            },
            match spec.relocation {
                Relocation::PositionIndependent => RelocMode::PIC,
                Relocation::Static => RelocMode::Static,
                Relocation::DynamicNoPic => RelocMode::DynamicNoPic,
            },
            match spec.code_model {
                CodeModelChoice::Default => CodeModel::Default,
                CodeModelChoice::JitDefault => CodeModel::JITDefault,
                CodeModelChoice::Small => CodeModel::Small,
                CodeModelChoice::Kernel => CodeModel::Kernel,
                CodeModelChoice::Medium => CodeModel::Medium,
                CodeModelChoice::Large => CodeModel::Large,
            },
        )
        .ok_or_else(|| {
            Diag::new(
                format!("`{}` is not a machine LLVM can emit for", triple_text),
                spec.span,
                "check the architecture, vendor, system and object format name a real machine",
            )
        })?;

    let properties = properties_of(&target_machine);
    Ok(Built { properties, target: target_machine, triple: triple_text })
}

fn registered_targets() -> Vec<String> {
    let mut out = Vec::new();
    let mut next = Target::get_first();
    while let Some(t) = next {
        out.push(t.get_name().to_string_lossy().to_string());
        next = t.get_next();
    }
    out
}

fn emitting_targets() -> Vec<String> {
    let mut out = Vec::new();
    let mut next = Target::get_first();
    while let Some(t) = next {
        if t.has_target_machine() {
            out.push(t.get_name().to_string_lossy().to_string());
        }
        next = t.get_next();
    }
    out
}

/// The nearest registered name, when one is close enough to be worth offering.
fn nearest(name: &str, known: &[String]) -> Option<String> {
    known
        .iter()
        .map(|k| (distance(name, k), k))
        .filter(|(d, _)| *d * 2 <= name.len().max(1))
        .min_by_key(|(d, _)| *d)
        .map(|(_, k)| k.clone())
}

fn distance(a: &str, b: &str) -> usize {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ac) in a.iter().enumerate() {
        let mut previous = row[0];
        row[0] = i + 1;
        for (j, bc) in b.iter().enumerate() {
            let replace = previous + usize::from(ac != bc);
            previous = row[j + 1];
            row[j + 1] = replace.min(row[j] + 1).min(row[j + 1] + 1);
        }
    }
    row[b.len()]
}

/// The one claim LLVM will not referee. Kept deliberately small, and an
/// architecture missing from it is refused rather than waved through.
fn expected_convention(arch: &str, os: &str) -> Option<&'static str> {
    Some(match arch {
        "aarch64" | "arm64" | "aarch64_be" | "aarch64_32" | "arm64_32" => "aapcs64",
        "x86-64" | "x86_64" => {
            if os.contains("windows") {
                "microsoft"
            } else {
                "systemv"
            }
        }
        "riscv64" => "lp64d",
        "riscv32" => "ilp32d",
        "wasm32" | "wasm64" => "wasm",
        _ => return None,
    })
}

fn properties_of(machine: &TargetMachine) -> Machine {
    let data = machine.get_target_data();
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
    Machine {
        triple: machine.get_triple().as_str().to_string_lossy().to_string(),
        pointer_bits: data.get_pointer_byte_size(None) * 8,
        little_endian: data.get_byte_ordering() == ByteOrdering::LittleEndian,
        max_alignment,
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
    // The strictest alignment any Verb-AL value asks for is computed rather
    // than assumed, so a program's claim is checked against the machine and
    // not against a table someone wrote down once.
    Ok(properties_of(&machine))
}
