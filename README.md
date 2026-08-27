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

Writing works the same way. A write declares which character classes its
literal content draws on, and that claim is checked too:

```
standard-output:print.string.space.comma.exclamation.newline-too.end:["Hello, World!"]end
```

Including the newline. A write ends with one only when it says `newline-too`,
because an automatic newline would be a character in the output that nothing in
the program accounts for.

Naming a variable means restating its declaration, so that the point of use says
again exactly what it uses — and the restatement is checked against the
declaration it claims to be:

```
privacy:local, memory:static, type:float.1-sign-bit.8-exponent-bits.23-explicit-mantissa-bits, name.string.space.comma.emoji.end: "Apples" = '200'end
standard-output:print.string.space.emoji.comma.colon.variable.newline-too.end:["Apples present: " connect with (privacy:local, memory:static, type:float.1-sign-bit.8-exponent-bits.23-explicit-mantissa-bits, name.string.space.comma.emoji.end: "Apples" = '200'end)]end
```

```
rule broke & where: naming a variable restates its declaration exactly; this restatement of "Apples" disagrees about its type
suggested fix: the declaration says float.1-sign-bit.8-exponent-bits.23-explicit-mantissa-bits
```

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
| [`hello-world.val`](examples/hello-world.val) | the first program |
| [`apples.val`](examples/apples.val) | writing a literal and a variable together |
| [`hello.val`](examples/hello.val) | the canonical declaration |
| [`counting.val`](examples/counting.val) | repetition and assignment |
| [`fizzbuzz.val`](examples/fizzbuzz.val) | nested branches, `remainder-of`, block scoping |
| [`memory.val`](examples/memory.val) | `static` and `automatic` telling themselves apart |
| [`layouts.val`](examples/layouts.val) | every type, printing at its declared precision |
| [`writing.val`](tests/programs/writing.val) | literal content and computed values in one write |
| [`newlines.val`](tests/programs/newlines.val) | building one line out of several writes |

`layouts.val` prints one tenth four times:

```
0.099976            half     — 5 exponent bits, 10 mantissa bits
0.1001              bfloat   — 8 and 7
0.100000001         single   — 8 and 23
0.10000000000000001 double   — 11 and 52
```

Four different numbers, because they are four different layouts, and Verb-AL
prints each at the shortest precision that round-trips it.

## The machine is not assumed either

Every program states, once, what it needs of the machine it is built for — and
the claim is checked against what LLVM says the machine is:

```
requires:target.64-bit-pointers.little-endian.8-byte-maximum-alignment end
```

Only the three properties the source can actually depend on. A CPU model or an
optimisation level changes nothing a program is allowed to claim, so it is no
business of the program's — it belongs to the build, in a `.machine` file
written in Verb-AL's own grammar:

```
machine:aarch64, cpu:apple-m1, features.neon.crypto.end, system:apple.darwin25.macho,
  calling-convention:aapcs64,
  optimisation:none,
  relocation:position-independent,
  code-model:small end
```

```bash
verbal build hello.val -m machines/mac-arm64.machine -o hello
```

`build` and `emit-ir` require one, because they produce something for a
machine. `run` and `jit` refuse one, because they execute here. `check` will
take one, and tells you when it was not given one — a clean exit that quietly
meant "clean among the claims I bothered to check" is the defect this language
is organised against.

## The compiler asks permission

Nothing is implicit — including whether the compiler may talk to you. A program
that has not permitted error messages does not get error messages. It fails,
and says nothing about why:

```bash
$ verbal check broken.val
$ echo $?
1
```

Opt in, and it explains itself:

```
allow[compiler:error.error-message]end
```

A grant covers everything beneath it, so `allow[compiler:error]end` does the
same. Permissions are read before compilation starts, from a tolerant scan that
reads past text it cannot lex — so a file whose opt-in is intact still gets its
diagnostics even when the rest of it does not tokenise. Runtime faults are not
covered by a `compiler:` grant; a program that divides by zero while running
still says so.

## Diagnostics

A report is six lines: a location your terminal will linkify, then the same
facts labelled one per line.

```
tests/errors/forbidden-class.val:4:49
file: tests/errors/forbidden-class.val
line: 4
column: 49
rule broke & where: a name uses only the classes its descriptor permits; this descriptor permits string.digit, but "well-formed" contains `-`, which is hyphen
suggested fix: write name.string.digit.hyphen.end
```

`rule broke & where` names the rule and the place that broke it. `suggested
fix` says what to write instead and is never empty — the diagnostic type cannot
be constructed without one, which makes unhelpfulness a compile error in the
compiler. Where a fix can be specific, it is:

```
rule broke & where: every name must be declared before it is used; "countor" never was
suggested fix: write "counter", which is declared and in scope
```

The same rule that governs names governs written content, so a write whose
descriptor forgets a class is told which character gave it away:

```
rule broke & where: what is written uses only the classes its descriptor permits; this descriptor permits string.space.emoji.comma, but "Hello, World!" contains `!`, which is exclamation
suggested fix: write print.string.space.emoji.comma.exclamation.newline-too.end
```

Recorded transcripts of every diagnostic live in [`tests/errors`](tests/errors);
re-record them with `VERBAL_BLESS=1 cargo test`. A test walks all of them and
insists on the full six lines.

## Not yet in the language

**Functions** — their syntax has not been decided, and guessing would be worse
than waiting. Also absent: lists and records, imports, explicit type
conversion, and reclaiming the memory `joined-with` allocates. None of these
requires revisiting the grammar.
