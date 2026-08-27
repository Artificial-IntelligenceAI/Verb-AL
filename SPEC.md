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

There are four kinds: declarations (§3), actions (§4), writes (§5) and
permissions (§6).

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
float declaration and the integer 200 under an integer declaration. With no
context to fix it — as in `action:say, source:'hello'end` — a literal is text.

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
    action:say, source:'big'end
  otherwise:
    action:say, source:'small'end
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
content draws on, says whether it ends in a newline, and lists what to write
between brackets.

The descriptor is the same construct as a name descriptor (§3.4) doing the same
job: `print.<classes>.end` **permits** the classes the literal content may draw
on, and is checked. It may permit classes the content never uses; it may not
omit one the content does use. The classes are those of §3.4 plus `plus`,
`newline`, `tab` and `carriage-return`, which literal content can contain and a
name in practice will not.

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
written, while `newline-too` appends one *after* it. A name descriptor rejects
`newline-too` outright, since a name is not written anywhere.

It may appear anywhere in the chain, like a class, but never twice.

Between the brackets, comma-separated, two kinds of thing:

| Written as | Is |
|---|---|
| `"…"` | literal character content, governed by the descriptor |
| `(…)` | a value — any expression (§7) — printed as its type prints (§8) |

A parenthesised value is not literal content, so the descriptor says nothing
about it: `standard-output:print.newline-too.end:[("count")]end` writes a number
under a descriptor that permits no classes at all, and is correct, because the
statement contains no literal characters. What a computed value prints is fixed by its type,
not by the program text.

Double quotes here mean what they mean in a declaration: a run of characters
whose composition is being declared. Single quotes are values, and a value
between the brackets must be parenthesised to say so.

## 6. Permissions

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

### 6.1 The shape of a report

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

## 7. Expressions

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

## 8. Runtime semantics

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

## 9. Deliberately absent from version 1

Functions — the user has not yet decided their syntax, and guessing would be
worse than waiting. Also absent: aggregates (lists, records), imports, explicit
type conversion, and freeing text (`joined-with` allocates and version 1 does
not reclaim). Each is additive; none requires revisiting the grammar above.
