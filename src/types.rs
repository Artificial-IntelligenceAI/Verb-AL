//! Verb-AL types, and the spelled-out descriptors that denote them.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FloatKind {
    Half,
    BFloat,
    Single,
    Double,
}

impl FloatKind {
    /// (exponent bits, explicit mantissa bits)
    pub fn layout(self) -> (u32, u32) {
        match self {
            FloatKind::Half => (5, 10),
            FloatKind::BFloat => (8, 7),
            FloatKind::Single => (8, 23),
            FloatKind::Double => (11, 52),
        }
    }

    pub fn from_layout(exponent: u32, mantissa: u32) -> Option<FloatKind> {
        match (exponent, mantissa) {
            (5, 10) => Some(FloatKind::Half),
            (8, 7) => Some(FloatKind::BFloat),
            (8, 23) => Some(FloatKind::Single),
            (11, 52) => Some(FloatKind::Double),
            _ => None,
        }
    }

    /// The shortest `%g` precision that round-trips this layout. See SPEC §8.
    pub fn print_precision(self) -> u32 {
        match self {
            FloatKind::Half => 5,
            FloatKind::BFloat => 4,
            FloatKind::Single => 9,
            FloatKind::Double => 17,
        }
    }

    /// Round a f64 through this layout's precision, so that the interpreter
    /// stores exactly what the compiled program would store.
    pub fn round(self, v: f64) -> f64 {
        match self {
            FloatKind::Double => v,
            FloatKind::Single => v as f32 as f64,
            FloatKind::Half => round_via_bits(v, 5, 10),
            FloatKind::BFloat => round_via_bits(v, 8, 7),
        }
    }
}

/// Round-trip a double through an arbitrary (exponent, mantissa) IEEE layout.
/// Used for `half` and `bfloat`, which Rust has no native type for.
fn round_via_bits(v: f64, exp_bits: u32, man_bits: u32) -> f64 {
    if !v.is_finite() {
        return v;
    }
    if v == 0.0 {
        return v;
    }
    let bias = (1i32 << (exp_bits - 1)) - 1;
    let min_exp = 1 - bias;
    let max_exp = bias;
    let (mant, exp) = frexp(v.abs());
    let sign = if v.is_sign_negative() { -1.0 } else { 1.0 };
    if exp - 1 > max_exp {
        return sign * f64::INFINITY;
    }
    // Number of significand bits available at this exponent (subnormals lose some).
    let avail = if exp - 1 < min_exp {
        (man_bits as i32 + 1) - (min_exp - (exp - 1))
    } else {
        man_bits as i32 + 1
    };
    if avail <= 0 {
        return sign * 0.0;
    }
    let scale = (2f64).powi(avail);
    let rounded = (mant * scale).round_ties_even() / scale;
    sign * ldexp(rounded, exp)
}

fn frexp(v: f64) -> (f64, i32) {
    let bits = v.to_bits();
    let raw_exp = ((bits >> 52) & 0x7ff) as i32;
    if raw_exp == 0 {
        // Subnormal: normalise by scaling up first.
        let (m, e) = frexp(v * (2f64).powi(64));
        return (m, e - 64);
    }
    let exp = raw_exp - 1022;
    let mant = f64::from_bits((bits & !(0x7ffu64 << 52)) | (1022u64 << 52));
    (mant, exp)
}

fn ldexp(m: f64, e: i32) -> f64 {
    m * (2f64).powi(e)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Type {
    Int { width: u32, signed: bool },
    Float(FloatKind),
    Truth,
    Character,
    Text,
}

impl Type {
    pub fn is_numeric(self) -> bool {
        matches!(self, Type::Int { .. } | Type::Float(_))
    }

    pub fn is_ordered(self) -> bool {
        self.is_numeric() || self == Type::Character
    }

    /// The canonical spelled-out descriptor for this type, used in error
    /// messages so the compiler always suggests text you could paste back.
    pub fn describe(self) -> String {
        match self {
            Type::Int { width, signed: true } => format!(
                "integer.1-sign-bit.{}.twos-complement",
                plural(width - 1, "value-bit")
            ),
            Type::Int { width, signed: false } => {
                format!("integer.0-sign-bits.{}.unsigned-binary", plural(width, "value-bit"))
            }
            Type::Float(k) => {
                let (e, m) = k.layout();
                format!(
                    "float.1-sign-bit.{}.{}",
                    plural(e, "exponent-bit"),
                    plural(m, "explicit-mantissa-bit")
                )
            }
            Type::Truth => "truth.1-bit".to_string(),
            Type::Character => "character.32-bits.unicode-scalar".to_string(),
            Type::Text => "text.utf-8.pointer-and-length".to_string(),
        }
    }
}

/// `1-value-bit` but `31-value-bits` — Verb-AL insists on grammatical agreement.
pub fn plural(n: u32, noun: &str) -> String {
    if n == 1 {
        format!("{}-{}", n, noun)
    } else {
        format!("{}-{}s", n, noun)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CharClass {
    String,
    Digit,
    Space,
    Comma,
    Period,
    Hyphen,
    Underscore,
    Apostrophe,
    Exclamation,
    Question,
    Colon,
    Slash,
    Plus,
    Newline,
    Tab,
    CarriageReturn,
    Emoji,
}

impl CharClass {
    pub fn word(self) -> &'static str {
        match self {
            CharClass::String => "string",
            CharClass::Digit => "digit",
            CharClass::Space => "space",
            CharClass::Comma => "comma",
            CharClass::Period => "period",
            CharClass::Hyphen => "hyphen",
            CharClass::Underscore => "underscore",
            CharClass::Apostrophe => "apostrophe",
            CharClass::Exclamation => "exclamation",
            CharClass::Question => "question",
            CharClass::Colon => "colon",
            CharClass::Slash => "slash",
            CharClass::Plus => "plus",
            CharClass::Newline => "newline",
            CharClass::Tab => "tab",
            CharClass::CarriageReturn => "carriage-return",
            CharClass::Emoji => "emoji",
        }
    }

    pub fn from_word(w: &str) -> Option<CharClass> {
        use CharClass::*;
        Some(match w {
            "string" => String,
            "digit" => Digit,
            "space" => Space,
            "comma" => Comma,
            "period" => Period,
            "hyphen" => Hyphen,
            "underscore" => Underscore,
            "apostrophe" => Apostrophe,
            "exclamation" => Exclamation,
            "question" => Question,
            "colon" => Colon,
            "slash" => Slash,
            "plus" => Plus,
            "newline" => Newline,
            "tab" => Tab,
            "carriage-return" => CarriageReturn,
            "emoji" => Emoji,
            _ => return None,
        })
    }
}

fn is_emoji(c: char) -> bool {
    let c = c as u32;
    matches!(c,
        0x200D                  // zero-width joiner
        | 0x2600..=0x27BF       // misc symbols, dingbats
        | 0x2B00..=0x2BFF       // misc symbols and arrows
        | 0xFE00..=0xFE0F       // variation selectors
        | 0x1F000..=0x1FAFF     // the pictographic planes
    )
}

pub fn classify(c: char) -> Option<CharClass> {
    use CharClass::*;
    Some(match c {
        ' ' => Space,
        ',' => Comma,
        '.' => Period,
        '-' => Hyphen,
        '_' => Underscore,
        '\'' => Apostrophe,
        '!' => Exclamation,
        '?' => Question,
        ':' => Colon,
        '/' => Slash,
        '+' => Plus,
        '\n' => Newline,
        '\t' => Tab,
        '\r' => CarriageReturn,
        _ if is_emoji(c) => Emoji,
        _ if c.is_alphabetic() => String,
        _ if c.is_numeric() => Digit,
        _ => return None,
    })
}

/// The classes present in `name`, deduplicated, in order of first appearance.
/// Returns `Err` at the first character belonging to no class.
pub fn classes_of(name: &str) -> Result<Vec<CharClass>, char> {
    let mut seen: Vec<CharClass> = Vec::new();
    for c in name.chars() {
        let class = classify(c).ok_or(c)?;
        if !seen.contains(&class) {
            seen.push(class);
        }
    }
    Ok(seen)
}

pub fn describe_classes(classes: &[CharClass]) -> String {
    classes.iter().map(|c| c.word()).collect::<Vec<_>>().join(".")
}

/// A whole print descriptor, written the canonical way: the permitted classes
/// in the order given, then `newline-too` if the write ends with one.
pub fn describe_print(classes: &[CharClass], newline: bool) -> String {
    let mut parts: Vec<&str> = classes.iter().map(|c| c.word()).collect();
    if newline {
        parts.push(NEWLINE_TOO);
    }
    parts.push("end");
    format!("print.{}", parts.join("."))
}

/// Not a character class: a print descriptor writes this to ask for a trailing
/// newline. Distinct from the `newline` class, which permits a newline
/// *within* what is written.
pub const NEWLINE_TOO: &str = "newline-too";
