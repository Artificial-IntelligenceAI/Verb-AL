# Verb-AL

A statically typed, natively compiled language in which **nothing is implicit**.

Java writes `private static float x = 200f;` and leaves four facts to its
specification: that `float` means IEEE-754 binary32, what that layout is, what
`private static` denotes, and which characters an identifier may hold. Verb-AL
makes you say all of it:

```
privacy:local, memory:static, type:float.1-sign-bit.8-exponent-bits.23-explicit-mantissa-bits, name.string.space.comma.emoji.end: "Help me, please 🤣" = '200'end
```

The type states its bit layout instead of taking a name. The declaration states
its visibility and its storage duration. The name states which character
classes it is permitted to draw on — `string`, `space`, `comma`, `emoji` — which
is how an identifier gets to hold a comma and an emoji without confusing the
comma-separated attribute list around it. Every one of those claims is checked.

[`SPEC.md`](SPEC.md) is the language definition.

## Two backends, one language

Verb-AL has a tree-walking interpreter and an LLVM native compiler, and the
project's central claim is that they are the same language. They share one
definition of every operator ([`src/value.rs`](src/value.rs)) and one definition
of output formatting ([`src/fmt.rs`](src/fmt.rs)) — both backends format through
the platform's `printf`, so their bytes are identical by construction rather
than by two careful reimplementations. `cargo test` runs the whole corpus
through both and insists on the same output, the same standard error and the
same exit status.

## Building

Needs Rust and LLVM 21. On this machine `.cargo/config.toml` already points
inkwell at Homebrew's keg-only `llvm@21`; elsewhere, set

```bash
export LLVM_SYS_211_PREFIX=$(brew --prefix llvm@21)
```

```bash
cargo build --release
```

## Running

```bash
verbal run examples/fizzbuzz.val        # interpret
verbal build examples/fizzbuzz.val      # compile to a native executable
verbal jit examples/fizzbuzz.val        # compile in memory and run
verbal emit-ir examples/fizzbuzz.val    # print the LLVM IR
verbal check examples/fizzbuzz.val      # report errors and stop
```

## Examples

| File | Shows |
|---|---|
| [`hello.val`](examples/hello.val) | the canonical declaration |
| [`counting.val`](examples/counting.val) | repetition and assignment |
| [`fizzbuzz.val`](examples/fizzbuzz.val) | nested branches, `remainder-of`, block scoping |
| [`memory.val`](examples/memory.val) | `static` and `automatic` telling themselves apart |
| [`layouts.val`](examples/layouts.val) | every type, printing at its declared precision |

`layouts.val` prints one tenth four times:

```
0.099976            half     — 5 exponent bits, 10 mantissa bits
0.1001              bfloat   — 8 and 7
0.100000001         single   — 8 and 23
0.10000000000000001 double   — 11 and 52
```

Four different numbers, because they are four different layouts, and Verb-AL
prints each at the shortest precision that round-trips it.

## Diagnostics

The descriptors earn their keep by being checked:

```
error: this name uses a class its descriptor does not permit
 --> tests/errors/forbidden-class.val:2:49
  |
2 | privacy:local, memory:static, type:truth.1-bit, name.string.digit.end: "well-formed" = 'true'end
  |                                                 ^^^^^^^^^^^^^^^^^^^^^
  = the descriptor permits: string.digit
  = but "well-formed" contains `-`, which is hyphen
  = write: name.string.digit.hyphen.end
```

A descriptor is an allowance rather than an inventory: it may permit classes
the name never uses, and the order it permits them in means nothing.

Recorded transcripts of every diagnostic live in [`tests/errors`](tests/errors);
re-record them with `VERBAL_BLESS=1 cargo test`.

## Not yet in the language

**Functions** — their syntax has not been decided, and guessing would be worse
than waiting. Also absent: lists and records, imports, explicit type
conversion, and reclaiming the memory `joined-with` allocates. None of these
requires revisiting the grammar.
