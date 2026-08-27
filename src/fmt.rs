//! Output formatting, defined once and reached identically by both backends.
//!
//! The interpreter formats through the platform's `snprintf`, and the compiled
//! program formats through the platform's `printf`, so the bytes they produce
//! are identical by construction rather than by careful reimplementation.

use crate::types::Type;
use crate::value::Value;

/// The printf conversion a type is written with. See SPEC §6.
pub fn conversion(ty: Type) -> String {
    match ty {
        Type::Int { signed: true, .. } => "%lld".to_string(),
        Type::Int { signed: false, .. } => "%llu".to_string(),
        Type::Float(kind) => format!("%.{}g", kind.print_precision()),
        Type::Truth | Type::Character | Type::Text => "%s".to_string(),
    }
}

fn c_snprintf(format: &str, apply: impl Fn(*mut libc::c_char, usize, *const libc::c_char) -> i32) -> Vec<u8> {
    let cformat = std::ffi::CString::new(format).expect("format holds no NUL");
    let mut buf = vec![0u8; 512];
    let written = apply(buf.as_mut_ptr() as *mut libc::c_char, buf.len(), cformat.as_ptr());
    let written = written.max(0) as usize;
    if written >= buf.len() {
        buf = vec![0u8; written + 1];
        let again = apply(buf.as_mut_ptr() as *mut libc::c_char, buf.len(), cformat.as_ptr());
        buf.truncate(again.max(0) as usize);
    } else {
        buf.truncate(written);
    }
    buf
}

/// The bytes `action:say` writes for this value, excluding the trailing newline.
pub fn say_bytes(value: &Value, ty: Type) -> Vec<u8> {
    match (value, ty) {
        (Value::Int(v), Type::Int { signed: true, .. }) => {
            let v = *v as i64;
            c_snprintf("%lld", |p, n, f| unsafe { libc::snprintf(p, n, f, v) })
        }
        (Value::Int(v), Type::Int { signed: false, .. }) => {
            let v = *v as u64;
            c_snprintf("%llu", |p, n, f| unsafe { libc::snprintf(p, n, f, v) })
        }
        (Value::Float(v), Type::Float(kind)) => {
            let v = *v;
            c_snprintf(&format!("%.{}g", kind.print_precision()), |p, n, f| unsafe {
                libc::snprintf(p, n, f, v)
            })
        }
        (Value::Truth(b), _) => if *b { b"true".to_vec() } else { b"false".to_vec() },
        (Value::Char(c), _) => c.to_string().into_bytes(),
        (Value::Text(s), _) => s.as_bytes().to_vec(),
        _ => unreachable!("value and type always agree after checking"),
    }
}
