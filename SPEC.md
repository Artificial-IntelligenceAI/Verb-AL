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
| `"double"` | an **identifier** — a name being declared, or a reference to one |
| `'single'` | a **value literal** |

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

The key `name` carries a dotted list of the character classes occurring in the
identifier: **deduplicated, in order of first appearance, exhaustive**, closed by
`.end`.

For `"Help me, please 🤣"` the characters run

```
H e l p   ␣   m e   ,   ␣   p l e a s e   ␣   🤣
string    space string comma …            …    emoji
```

whose first appearances in order are string, space, comma, emoji — hence
`name.string.space.comma.emoji.end`.

The descriptor is **checked**. If it does not match the name exactly, the
program does not compile, and the error tells you the descriptor you should
have written.

| Class | Characters |
|---|---|
| `string` | any Unicode alphabetic character |
| `digit` | any Unicode decimal digit |
| `space` | U+0020 |
| `comma` `period` `hyphen` `underscore` | `,` `.` `-` `_` |
| `apostrophe` `exclamation` `question` `colon` `slash` | `'` `!` `?` `:` `/` |
| `emoji` | pictographic scalars, variation selectors and zero-width joiners |

An empty name is a compile error, so every descriptor lists at least one class.

## 4. Actions

```
action:note, remark:'…'end                       — a comment; evaluated by nobody
action:say, source:<expr>end                     — write the value and a newline to standard output
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

## 5. Expressions

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

## 6. Runtime semantics

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

## 7. Deliberately absent from version 1

Functions — the user has not yet decided their syntax, and guessing would be
worse than waiting. Also absent: aggregates (lists, records), imports, explicit
type conversion, and freeing text (`joined-with` allocates and version 1 does
not reclaim). Each is additive; none requires revisiting the grammar above.
