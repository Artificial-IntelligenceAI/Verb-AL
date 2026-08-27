# Verb-AL — Language Specification, version 1

Verb-AL is a statically typed, natively compiled language whose organising
principle is **nothing is implicit**. Where another language lets the
specification carry a fact, Verb-AL makes you say it.

Java writes:

```java
private static float x = 200f;
```

and leaves four things to the spec: that `float` means IEEE-754 binary32, that
its layout is one sign bit / eight exponent bits / twenty-three stored mantissa
bits, that `private` and `static` are the visibility and storage class, and that
`x` is drawn from a permitted identifier alphabet. Verb-AL writes all of it:

```
privacy:local, memory:static, type:float.1-sign-bit.8-exponent-bits.23-explicit-mantissa-bits, name.string.space.comma.emoji.end: "Help me, please 🤣" = '200'end
```

That line is the canonical example of the language and every rule below is
visible in it.

---

## 1. The shape of a statement

Every statement in Verb-AL, without exception, is a comma-separated list of
`key:value` attributes terminated by the word `end`. There is one grammar, not
several. Declarations and actions differ only in which keys they carry.

```
statement := attribute ("," attribute)* terminator
attribute := key [descriptor] ":" value
```

There are five kinds: declarations (§3), actions (§4), writes (§5),
requirements (§6) and permissions (§7).

Two kinds of dotted **descriptor** appear, and the difference is deliberate:

| Form | Meaning | Example |
|---|---|---|
| descriptor **is** the value | the attribute's whole content is a spelled-out description | `type:float.1-sign-bit.8-exponent-bits.23-explicit-mantissa-bits` |
| descriptor **constrains** the value | attached to the *key*, it declares the shape of the value that follows | `name.string.space.comma.emoji.end: "Help me, please 🤣"` |

A descriptor attached to a key is closed by `.end` so the parser knows where the
chain stops and the value begins.

## 2. Quoting

| Quote | Means |
|---|---|
| `"double"` | a run of characters whose composition is declared — a name (§3.4), a reference to one, or literal content being written (§5) |
| `'single'` | a **value literal** |

Double quotes always surround characters the program is talking about *as
characters*, which is why a class descriptor attaches to them and why they are
checked against it. Single quotes surround a value.

The split exists so that a name may contain spaces, commas, emoji or anything
else without ever colliding with the punctuation of the attribute list.
`"Help me, please 🤣"` holds a comma; the attribute list is not confused,
because the comma is inside double quotes.

Both quote forms accept the escapes `\\` `\'` `\"` `\n` `\t` `\r` `\0`
and `\u{...}` (hexadecimal Unicode scalar).

A literal's **type comes from its context**: `'200'` is the float 200.0 under a
float declaration and the integer 200 under an integer declaration. Every
position a literal may occupy fixes one — an initializer takes the declared
type, an assignment takes its target's, a condition is a truth, an operand
takes the type of what it is combined with — so a literal is never left to
guess. Where nothing fixes a type, as in a comparison of two literals, that is
itself the error, rather than a default being chosen quietly.

## 3. Declarations

```
privacy:<privacy>, memory:<memory>, type:<type>, name<namedesc>: "NAME" = <expr>end
```

All four attributes are **mandatory** and appear in **exactly this order**. There
are no defaults; omitting one is a compile error. The initializer is mandatory
too — there is no such thing as an uninitialized variable in Verb-AL.

### 3.1 `privacy`

| Value | Meaning |
|---|---|
| `local` | visible only within this compilation unit (LLVM internal linkage) |
| `public` | exported (LLVM external linkage) |

### 3.2 `memory`

| Value | Meaning |
|---|---|
| `static` | one storage cell for the whole program run, initialized **once** before the program begins |
| `automatic` | storage belonging to the enclosing block, initialized **each time** the declaration is reached |

The distinction is observable. A `static` declaration inside a loop body keeps
its value across iterations; an `automatic` one is reset every iteration. Both
backends implement this identically and a differential test pins it.

Because a static cell is filled before the program begins, **its initializer
must be constant** — it may combine literals, but it may not read a variable.
The compiler folds it at build time and the interpreter uses the folded value,
so the two cannot disagree about what a global started as. A fault in a static
initializer (`'1' divided-by '0'`) is therefore a compile error, not a
runtime one.

### 3.3 `type` — physical descriptors

A type is never merely named; it is described. The descriptor *is* the type, and
maps one-to-one onto an LLVM type with no table of aliases in between.

```
integer . <n>-sign-bit[s] . <m>-value-bit[s] . <encoding>
```

`<n>` is `0` or `1`. When it is `1` the encoding must be `twos-complement`; when
it is `0` it must be `unsigned-binary`. Total width is `n + m` and must fall in
1..=64. Singular and plural must agree with the number — `1-sign-bit`,
`0-sign-bits`, `31-value-bits`, `1-value-bit`. Getting the grammar wrong is a
compile error. RIP to the writers.

```
float . 1-sign-bit . <e>-exponent-bits . <m>-explicit-mantissa-bits
```

The mantissa count is the number of *stored* bits; the implicit leading one is
not yours to count. Four layouts exist:

| e | m | LLVM |
|---|---|---|
| 5 | 10 | `half` |
| 8 | 7 | `bfloat` |
| 8 | 23 | `float` |
| 11 | 52 | `double` |

```
truth . 1-bit
character . 32-bits . unicode-scalar
text . utf-8 . pointer-and-length
```

Examples:

```
type:integer.1-sign-bit.31-value-bits.twos-complement      → i32
type:integer.0-sign-bits.8-value-bits.unsigned-binary      → i8, unsigned
type:integer.1-sign-bit.63-value-bits.twos-complement      → i64
type:float.1-sign-bit.11-exponent-bits.52-explicit-mantissa-bits → double
type:truth.1-bit                                           → i1
```

### 3.4 `name` — the character-class descriptor

The key `name` carries a dotted list of the character classes the identifier is
**permitted** to draw from, closed by `.end`. It is an allowance, not an
inventory: the name must use only classes the descriptor lists, but it need not
use every class it lists.

```
name.string.space.comma.emoji.end: "Help me, please 🤣"
```

permits letters, spaces, commas and emoji, and this particular name happens to
use all four. The same name would be equally legal under a descriptor that also
permitted `digit`; a name of plain letters would be legal under this one.

The descriptor is **checked**. A name containing a character from a class the
descriptor does not permit does not compile, and the error names the offending
character and prints the descriptor you should have written.

Order is not significant, because a set of permissions has no order. No class
may be listed twice, and the list may not be empty — a name permitted nothing
could not be written at all.

| Class | Characters |
|---|---|
| `string` | any Unicode alphabetic character |
| `digit` | any Unicode decimal digit |
| `space` | U+0020 |
| `comma` `period` `hyphen` `underscore` | `,` `.` `-` `_` |
| `apostrophe` `exclamation` `question` `colon` `slash` | `'` `!` `?` `:` `/` |
| `emoji` | pictographic scalars, variation selectors and zero-width joiners |

## 4. Actions

```
action:note, remark:'…'end                       — a comment; evaluated by nobody
action:assign, target:"NAME", value:<expr>end    — store into an existing variable
```

### 4.1 Branch

`then:` and `otherwise:` introduce statement sequences. Each inner statement
carries its own terminator, so the parser always knows it is at a statement
boundary; the branch itself is closed by the named terminator `end-branch`.
`otherwise:` is optional.

```
action:branch, condition:<expr>,
  then:
    standard-output:print.string.newline-too.end:["big"]end
  otherwise:
    standard-output:print.string.newline-too.end:["small"]end
  end-branch
```

### 4.2 Repetition

```
action:repetition, while:<expr>,
  do:
    action:assign, target:"count", value:("count" plus '1')end
  end-repetition
```

A block introduces a scope. Names declared inside it leave scope at
`end-branch` / `end-repetition`, whatever their `memory` class. Shadowing an
existing name is a compile error.

## 5. Writing to standard output

```
standard-output:print.string.space.comma.exclamation.newline-too.end:["Hello, World!"]end
```

A write names its destination, declares which character classes its literal
content draws on, says whether it names a variable and whether it ends in a
newline, and lists what to write between brackets.

The descriptor is the same construct as a name descriptor (§3.4) doing the same
job: `print.<classes>.end` **permits** the classes the literal content may draw
on, and is checked. It may permit classes the content never uses; it may not
omit one the content does use. The classes are those of §3.4 plus `plus`,
`newline`, `tab` and `carriage-return`, which literal content can contain and a
name in practice will not.

Between the brackets, joined by `connect with`, two kinds of thing:

| Written as | Is |
|---|---|
| `"…"` | literal character content, governed by the descriptor |
| `(…)` | a variable, named by restating its declaration — §5.2 |

Double quotes here mean what they mean in a declaration: a run of characters
whose composition is being declared.

Besides the classes, a descriptor may carry two words that are not classes:
`variable` (§5.2) and `newline-too` (§5.1). Each may appear anywhere in the
chain but never twice, and neither may appear in a *name* descriptor, since both
say something about a write and a name is not written.

### 5.1 `newline-too`

A write ends with a newline only when its descriptor says `newline-too`. There
is no automatic newline, because an automatic newline is a character in the
output that nothing in the program accounts for.

```
standard-output:print.string.end:["no"]end
standard-output:print.string.end:["newline"]end
standard-output:print.string.newline-too.end:["between"]end
```

writes `nonewlinebetween` and one newline. That is how a line is built from
parts. An empty write asking for a newline —
`standard-output:print.newline-too.end:[]end` — is a blank line.

`newline-too` is **not** a character class, and the two neighbouring words mean
different things: the `newline` class permits a newline *within* what is
written, while `newline-too` appends one *after* it.

### 5.2 `variable` — naming one by restating it

A write may name a variable only if its descriptor says `variable`, and it names
one by **restating that variable's declaration in full**:

```
privacy:local, memory:static, type:float.1-sign-bit.8-exponent-bits.23-explicit-mantissa-bits, name.string.space.comma.emoji.end: "Apples" = '200'end
standard-output:print.string.space.emoji.comma.colon.variable.newline-too.end:["Apples present: " connect with (privacy:local, memory:static, type:float.1-sign-bit.8-exponent-bits.23-explicit-mantissa-bits, name.string.space.comma.emoji.end: "Apples" = '200'end)]end
```

At the point of use, the program says again exactly what it is using. The
restatement is **checked against the declaration it claims to be**: privacy,
memory, type, name descriptor, name and initial value must all agree, or the
program does not compile and the report says which attribute disagrees.
Requiring the restatement would be pointless if restating it differently were
not caught.

Name descriptors are compared as sets, since order in a descriptor carries no
meaning. The variable found is the one in scope at the write, so two sibling
blocks may each declare `"inner"` and each restate their own.

What a variable prints is fixed by its type (§9), not by the program text, so
the character classes say nothing about it. `print.variable.newline-too.end`
permits no classes at all and is correct for a write with no literal content.

A computed value is not a variable. To write one, declare a variable for it:

```
privacy:local, memory:automatic, type:truth.1-bit, name.string.space.end: "is large" = ("size" greater-than '0')end
```

## 6. Requirements

```
requires:target.64-bit-pointers.little-endian.8-byte-maximum-alignment end
```

Every program states, exactly once, what it requires of the machine it is built
for. This is not optional: a program that declined to say would leave the
compiler free to build for whatever machine it happened to be running on, and
the machine was the last fact in Verb-AL left to whoever ran the compiler.

Only three properties may be required, because only three affect anything the
source can say: how wide a pointer is, which end of a number comes first, and
how strictly a value must be aligned. A CPU model, an instruction-set extension,
a relocation mode and an optimisation level change nothing a program is allowed
to claim, so they are no business of the program's. They belong to the build.

The claim is **checked** against the machine, which LLVM is asked about rather
than a table being consulted. A program requiring 32 bits on a 64-bit machine
does not compile, and the report says what the machine actually is.

Note two departures from §3.3, both deliberate:

- **No plural agreement.** `64-bit-pointers` and `8-byte-maximum-alignment` are
  compound adjectives; the number describes the noun rather than counting it.
  §3.3's `31-value-bits` counts bits, so it agrees; `8-byte` does not count
  bytes, so it does not.
- **The terminator is preceded by a space.** This is the only statement whose
  `end` would otherwise abut a bare word, and `…alignmentend` is one word to any
  lexer. Everywhere else `end` follows a quote or a bracket and needs no space.

### 6.1 The machine file

A program says what it *requires*; a **machine file** says what a machine *is*.
It holds exactly one statement and nothing else, written in Verb-AL's own
grammar — a second syntax inside one project would be its own implicitness.

```
machine:aarch64, cpu:apple-m1, features.neon.crypto.end, system:apple.darwin25.macho,
  calling-convention:aapcs64,
  optimisation:none,
  relocation:position-independent,
  code-model:small end
```

One statement, not several: splitting it would leave which clauses belong to
the same machine a fact nobody states. A `.val` file may not contain a
`machine:` statement, and a machine file may contain nothing else; each
rejection is an error in the ordinary shape, so neither file can quietly become
the other.

| Clause | Is |
|---|---|
| `machine:` | the architecture, named as LLVM names it |
| `cpu:` | the processor |
| `features.…end` / `no-extra-features` | instruction-set extensions, an allowance like §3.4 — with the empty case said aloud rather than written as an empty list |
| `system:` | vendor, operating system (version included), object format |
| `calling-convention:` | how arguments are passed |
| `optimisation:` | `none`, `less`, `default` or `aggressive` |
| `relocation:` | `position-independent`, `static` or `dynamic-no-pic` |
| `code-model:` | `small`, `kernel`, `medium`, `large`, `default` or `jit-default` |

It states **none** of the three properties a program may require. Naming the
architecture settles all three, and restating them here would be the
architecture-to-properties table §3.3 refuses for types. The program states
requirements, the machine names a machine, and LLVM referees between them. That
is also why byte order is a requirement but not a machine clause: a bi-endian
architecture distinguishes itself in its own name, `aarch64` against
`aarch64_be`, while a program never names an architecture at all.

Names are LLVM's throughout — `aarch64` rather than "ARM64", `macho` rather
than "Mach-O" — because LLVM is the referee and a translation layer would be
another table. An architecture LLVM does not know is an error listing the ones
it does; an architecture it knows but cannot emit for is a *different* error,
since knowing a name and being able to use it are different facts.

There is exactly one claim LLVM will not referee, and so exactly one table this
compiler carries: the calling convention. It is checked against what the
architecture and system imply, and an architecture the table does not cover is
refused rather than waved through, so the table's edge is an error rather than
a silence.

Note `optimisation:none` in the example. There is no level named "default" that
is also the default: an optimisation level can change floating-point results,
and §9 promises the interpreter and the compiled program agree bit for bit, so
the level is a thing the build states rather than a thing that happens.

### 6.2 Which commands name a machine

Whether a command takes a machine follows from whether it produces something
for one:

| Command | Machine |
|---|---|
| `build`, `emit-ir` | **required** — they produce an artefact for a machine |
| `run`, `jit` | **refused** — they execute here, so the machine is this one |
| `check` | optional, and it says which it used |

```
$ verbal check hello.val
hello.val: no errors, checked against this host (arm64-apple-darwin25.6.0) — pass -m …

$ verbal check hello.val -m machines/mac-arm64.machine
hello.val: no errors, checked for aarch64-apple-darwin25-macho
```

`check` may not exit cleanly without saying what it checked against. A clean
exit that quietly meant "clean among the claims I bothered to check" is the
defect this language is organised against.

`run` refuses a machine rather than interpreting as though it were one: the
interpreter's arithmetic is this machine's arithmetic, and pretending otherwise
would be the drift §9's promise exists to catch.

## 7. Permissions

The compiler does not assume it may speak. A program that has not permitted
error messages does not receive error messages: it fails, with a status and
nothing else.

```
allow[compiler:error.error-message]end
```

Permission is a statement like any other — `allow`, a bracketed permission, and
`end`. Inside the brackets, a subject and a dotted path saying what the subject
may do.

A grant covers itself and everything beneath it, so `allow[compiler:error]end`
also permits `compiler:error.error-message`. Version 1 recognises exactly those
two; anything else in the brackets is an error, which of course you will only be
told about if some other grant has already permitted it.

Permissions are read **before compilation begins**, from a tolerant scan of the
whole file, because whether the compiler may explain itself must be settled
before it discovers that it wants to. That scan reads past text it cannot
lex, so a file whose opt-in is intact still gets its diagnostics even when the
rest of it does not tokenise. Position in the file does not matter; a permission
applies to the whole of it.

Runtime faults are not covered. `compiler:` grants concern the compiler, and a
program that divides by zero while running still says so.

### 7.1 The shape of a report

A permitted diagnostic is six lines: a location a terminal will turn into a
link, then the same facts spelled out and labelled, because a report that made
you infer its own structure would be a poor advertisement for the language.

```
tests/errors/forbidden-class.val:4:49
file: tests/errors/forbidden-class.val
line: 4
column: 49
rule broke & where: a name uses only the classes its descriptor permits; this descriptor permits string.digit, but "well-formed" contains `-`, which is hyphen
suggested fix: write name.string.digit.hyphen.end
```

`rule broke & where` names the rule that was violated and the place in the
program that violated it — not merely what the compiler wanted to see.
`suggested fix` says what to write instead, and is **never empty**: the
diagnostic type cannot be constructed without one, so a rule with no
mechanical remedy has to say what to do about it in words. A test walks every
recorded diagnostic and insists on all six lines.

Compilation stops at the first broken rule, so a report describes one.

## 8. Expressions

```
expr    := primary | unary primary | primary binary primary
primary := "NAME" | 'literal' | "(" expr ")"
```

**An expression holds at most one operator.** Anything larger must be
parenthesised. Verb-AL has no precedence table because a precedence table is a
fact left implicit, and this language does not leave facts implicit:

```
("a" times "b") plus "c"      ✓
"a" times "b" plus "c"        ✗  error: this expression applies two operators
                                   without saying which happens first
```

| Operator | Operands | Result |
|---|---|---|
| `plus` `minus` `times` `divided-by` | two of the same numeric type | that type |
| `remainder-of` | two of the same integer type | that type |
| `equal-to` `not-equal-to` | two of the same type | truth |
| `less-than` `greater-than` `at-least` `at-most` | two of the same numeric or character type | truth |
| `and` `or` | two truths | truth |
| `joined-with` | two texts | text |
| `not` (prefix) | a truth | truth |
| `negated` (prefix) | a number | that type |

There is **no implicit conversion of any kind**. An integer and a float may not
meet in the same operator.

## 9. Runtime semantics

**Integer arithmetic wraps** at the declared width, signed and unsigned alike;
there is no undefined behaviour. LLVM `add`/`sub`/`mul` are emitted without
`nsw`/`nuw` and the interpreter masks to width, so the two agree bit for bit.

**Division by zero**, and the signed overflow case `minimum divided-by -1`, are
**runtime errors**: a message on standard error and exit status 3. The compiler
emits the guard; the interpreter performs the same check.

**Output formatting is defined exactly**, because two backends that print
differently are two languages. Both backends format through the platform's
`snprintf`, so the bytes are identical by construction:

| Type | Format |
|---|---|
| signed integer | `%lld` |
| unsigned integer | `%llu` |
| `half` / `bfloat` / `float` / `double` | `%.5g` / `%.4g` / `%.9g` / `%.17g` — the shortest precision that round-trips that layout |
| truth | `true` / `false` |
| character | its UTF-8 encoding |
| text | its bytes |

A program that runs off the end of its statements exits 0.

Object-file symbols cannot hold spaces or emoji, so a declaration is emitted
under `verbal.<index>.<ascii echo of the name>`. This matters only to
`privacy:public`, whose exported symbol is that mangled form rather than the
name as written.

## 10. Deliberately absent from version 1

Functions — the user has not yet decided their syntax, and guessing would be
worse than waiting. Also absent: aggregates (lists, records), imports, explicit
type conversion, and freeing text (`joined-with` allocates and version 1 does
not reclaim). Each is additive; none requires revisiting the grammar above.
