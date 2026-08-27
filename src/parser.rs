//! Verb-AL's one and only statement grammar: a comma-separated list of
//! `key:value` attributes, terminated by a word.

use crate::ast::*;
use crate::diag::{Diag, Span};
use crate::lexer::{Tok, Token};
use crate::types::{CharClass, FloatKind, Type};

/// Said whenever a declaration's attributes arrive out of order or short.
const ORDER: &str =
    "a declaration states privacy, memory, type and name in that order, omitting none";

pub fn parse(tokens: &[Token]) -> Result<Vec<Stmt>, Diag> {
    let mut p = Parser { tokens, pos: 0 };
    let mut out = Vec::new();
    while !p.at_eof() {
        out.push(p.statement()?);
    }
    Ok(out)
}

struct Parser<'a> {
    tokens: &'a [Token],
    pos: usize,
}

impl<'a> Parser<'a> {
    fn peek(&self) -> &Tok {
        &self.tokens[self.pos].tok
    }
    fn peek_span(&self) -> Span {
        self.tokens[self.pos].span
    }
    fn at_eof(&self) -> bool {
        matches!(self.peek(), Tok::Eof)
    }
    fn bump(&mut self) -> &'a Token {
        let t = &self.tokens[self.pos];
        if self.pos + 1 < self.tokens.len() {
            self.pos += 1;
        }
        t
    }
    fn peek_word(&self) -> Option<&str> {
        match self.peek() {
            Tok::Word(w) => Some(w.as_str()),
            _ => None,
        }
    }

    fn expect(&mut self, tok: Tok, context: &str) -> Result<Span, Diag> {
        if *self.peek() == tok {
            Ok(self.bump().span)
        } else {
            Err(Diag::new(
                format!(
                    "a statement's punctuation is fixed: {} belongs {}, but {} is here instead",
                    tok.describe(),
                    context,
                    self.peek().describe()
                ),
                self.peek_span(),
                format!("write {} here", tok.describe()),
            ))
        }
    }

    fn expect_word(&mut self, word: &str, context: &str) -> Result<Span, Diag> {
        if self.peek_word() == Some(word) {
            Ok(self.bump().span)
        } else {
            Err(Diag::new(
                format!(
                    "`{}` belongs {}, but {} is here instead",
                    word,
                    context,
                    self.peek().describe()
                ),
                self.peek_span(),
                format!("write `{}` here", word),
            ))
        }
    }

    fn any_word(&mut self, context: &str) -> Result<(String, Span), Diag> {
        match self.peek().clone() {
            Tok::Word(w) => {
                let span = self.bump().span;
                Ok((w, span))
            }
            other => Err(Diag::new(
                format!("{} belongs here, but {} is here instead", context, other.describe()),
                self.peek_span(),
                format!("write {} here", context),
            )),
        }
    }

    fn ident(&mut self, context: &str) -> Result<(String, Span), Diag> {
        match self.peek().clone() {
            Tok::Ident(n) => {
                let span = self.bump().span;
                Ok((n, span))
            }
            other => Err(Diag::new(
                format!(
                    "a name is written in double quotes; {} is here instead, where {} belongs",
                    other.describe(),
                    context
                ),
                self.peek_span(),
                "put the name in \"double quotes\" — 'single quotes' mean a value",
            )),
        }
    }

    // ---- statements -------------------------------------------------------

    fn statement(&mut self) -> Result<Stmt, Diag> {
        match self.peek_word() {
            Some("privacy") => self.declaration(),
            Some("action") => self.action(),
            Some("allow") => self.permission(),
            _ => Err(Diag::new(
                format!(
                    "every statement begins with `privacy`, `action` or `allow`; this one begins with {}",
                    self.peek().describe()
                ),
                self.peek_span(),
                "begin with `privacy:` to declare, `action:` to act, or `allow[` to permit",
            )),
        }
    }

    /// `allow[compiler:error.error-message]end`
    fn permission(&mut self) -> Result<Stmt, Diag> {
        let start = self.expect_word("allow", "to begin a permission")?;
        self.expect(Tok::LBracket, "after `allow`")?;
        let (subject, subject_span) =
            self.any_word("the subject being permitted, such as `compiler`")?;
        self.expect(Tok::Colon, "after the subject")?;
        let (head, head_span) = self.any_word("what the subject is permitted to do")?;
        let mut path = head;
        let mut span = subject_span.to(head_span);
        while *self.peek() == Tok::Dot {
            self.bump();
            let (part, part_span) = self.any_word("another part of the permission")?;
            path.push('.');
            path.push_str(&part);
            span = span.to(part_span);
        }
        self.expect(Tok::RBracket, "to close the permission")?;
        let end = self.expect_word("end", "to close the permission statement")?;
        Ok(Stmt::Allow {
            permission: format!("{}:{}", subject, path),
            permission_span: span,
            span: start.to(end),
        })
    }

    fn declaration(&mut self) -> Result<Stmt, Diag> {
        let start = self.expect_word("privacy", "to begin a declaration")?;
        self.expect(Tok::Colon, "after `privacy`")?;
        let (word, span) = self.any_word("`local` or `public`")?;
        let privacy = match word.as_str() {
            "local" => Privacy::Local,
            "public" => Privacy::Public,
            _ => {
                return Err(Diag::new(
                    format!("privacy is `local` or `public`; `{}` is neither", word),
                    span,
                    "write `local` to keep it to this file, or `public` to export it",
                ))
            }
        };

        self.expect(Tok::Comma, "after the privacy")?;
        self.expect_word("memory", "as the second attribute of a declaration")
            .map_err(|d| d.also(ORDER))?;
        self.expect(Tok::Colon, "after `memory`")?;
        let (word, span) = self.any_word("`static` or `automatic`")?;
        let memory = match word.as_str() {
            "static" => MemoryClass::Static,
            "automatic" => MemoryClass::Automatic,
            _ => {
                return Err(Diag::new(
                    format!("memory is `static` or `automatic`; `{}` is neither", word),
                    span,
                    "write `static` for one cell for the whole run, or `automatic` for a cell per block entry",
                ))
            }
        };

        self.expect(Tok::Comma, "after the memory class")?;
        self.expect_word("type", "as the third attribute of a declaration")
            .map_err(|d| d.also(ORDER))?;
        self.expect(Tok::Colon, "after `type`")?;
        let (ty, ty_span) = self.type_descriptor()?;

        self.expect(Tok::Comma, "after the type")?;
        let (name_classes, name_desc_span) = self.name_descriptor()?;
        self.expect(Tok::Colon, "after the name descriptor")?;
        let (name, name_span) = self.ident("the name being declared")?;
        self.expect(Tok::Equals, "before the initial value")?;
        let init = self.expression()?;
        let end = self.expect_word("end", "to close the declaration")?;

        Ok(Stmt::Decl(Decl {
            privacy,
            memory,
            ty,
            ty_span,
            name_classes,
            name_desc_span,
            name,
            name_span,
            init,
            span: start.to(end),
        }))
    }

    /// A dotted chain such as `float.1-sign-bit.8-exponent-bits.23-explicit-mantissa-bits`.
    fn type_descriptor(&mut self) -> Result<(Type, Span), Diag> {
        let (head, head_span) = self.any_word("a type descriptor")?;
        let mut parts: Vec<(String, Span)> = Vec::new();
        let mut span = head_span;
        while *self.peek() == Tok::Dot {
            self.bump();
            let (w, s) = self.any_word("another part of the type descriptor")?;
            span = span.to(s);
            parts.push((w, s));
        }
        let ty = build_type(&head, head_span, &parts, span)?;
        Ok((ty, span))
    }

    /// `name.string.space.comma.emoji.end`
    fn name_descriptor(&mut self) -> Result<(Vec<CharClass>, Span), Diag> {
        let start = self.expect_word("name", "as the fourth attribute of a declaration")
            .map_err(|d| d.also(ORDER))?;
        let mut classes = Vec::new();
        let mut span = start;
        loop {
            self.expect(Tok::Dot, "in the name descriptor")?;
            let (w, s) = self.any_word("a character class, or `end` to close the descriptor")?;
            span = span.to(s);
            if w == "end" {
                break;
            }
            let Some(class) = CharClass::from_word(&w) else {
                return Err(Diag::new(
                    format!("a name descriptor permits character classes; `{}` is not one", w),
                    s,
                    "use one of string, digit, space, comma, period, hyphen, underscore, \
                     apostrophe, exclamation, question, colon, slash, emoji",
                ));
            };
            if classes.contains(&class) {
                return Err(Diag::new(
                    format!("a descriptor is a set, so each class is permitted once; `{}` appears twice", w),
                    s,
                    format!("delete this second `.{}`", w),
                ));
            }
            classes.push(class);
        }
        if classes.is_empty() {
            return Err(Diag::new(
                "a name descriptor permits at least one class, since a name permitted nothing could not be written",
                span,
                "permit what the name uses, for instance name.string.end",
            ));
        }
        Ok((classes, span))
    }

    fn action(&mut self) -> Result<Stmt, Diag> {
        let start = self.expect_word("action", "to begin an action")?;
        self.expect(Tok::Colon, "after `action`")?;
        let (verb, verb_span) = self.any_word("the name of an action")?;

        match verb.as_str() {
            "note" => {
                self.expect(Tok::Comma, "after `note`")?;
                self.expect_word("remark", "as the second attribute of a note")?;
                self.expect(Tok::Colon, "after `remark`")?;
                let remark = match self.peek().clone() {
                    Tok::Lit(v) => {
                        self.bump();
                        v
                    }
                    other => {
                        return Err(Diag::new(
                            format!("a remark is a single-quoted value; {} is here instead", other.describe()),
                            self.peek_span(),
                            "put the remark in 'single quotes'",
                        ))
                    }
                };
                let end = self.expect_word("end", "to close the note")?;
                Ok(Stmt::Note { remark, span: start.to(end) })
            }

            "say" => {
                self.expect(Tok::Comma, "after `say`")?;
                self.expect_word("source", "as the second attribute of a say")?;
                self.expect(Tok::Colon, "after `source`")?;
                let source = self.expression()?;
                let end = self.expect_word("end", "to close the say")?;
                Ok(Stmt::Say { source, span: start.to(end) })
            }

            "assign" => {
                self.expect(Tok::Comma, "after `assign`")?;
                self.expect_word("target", "as the second attribute of an assign")?;
                self.expect(Tok::Colon, "after `target`")?;
                let (target, target_span) = self.ident("the name being assigned to")?;
                self.expect(Tok::Comma, "after the target")?;
                self.expect_word("value", "as the third attribute of an assign")?;
                self.expect(Tok::Colon, "after `value`")?;
                let value = self.expression()?;
                let end = self.expect_word("end", "to close the assign")?;
                Ok(Stmt::Assign { target, target_span, value, span: start.to(end) })
            }

            "branch" => {
                self.expect(Tok::Comma, "after `branch`")?;
                self.expect_word("condition", "as the second attribute of a branch")?;
                self.expect(Tok::Colon, "after `condition`")?;
                let cond = self.expression()?;
                self.expect(Tok::Comma, "after the condition")?;
                self.expect_word("then", "to introduce what happens when the condition holds")?;
                self.expect(Tok::Colon, "after `then`")?;
                let then = self.block(&["otherwise", "end-branch"], "end-branch", start)?;
                let mut otherwise = Vec::new();
                if self.peek_word() == Some("otherwise") {
                    self.bump();
                    self.expect(Tok::Colon, "after `otherwise`")?;
                    otherwise = self.block(&["end-branch"], "end-branch", start)?;
                }
                let end = self.expect_word("end-branch", "to close the branch")?;
                Ok(Stmt::Branch { cond, then, otherwise, span: start.to(end) })
            }

            "repetition" => {
                self.expect(Tok::Comma, "after `repetition`")?;
                self.expect_word("while", "as the second attribute of a repetition")?;
                self.expect(Tok::Colon, "after `while`")?;
                let cond = self.expression()?;
                self.expect(Tok::Comma, "after the condition")?;
                self.expect_word("do", "to introduce the body of the repetition")?;
                self.expect(Tok::Colon, "after `do`")?;
                let body = self.block(&["end-repetition"], "end-repetition", start)?;
                let end = self.expect_word("end-repetition", "to close the repetition")?;
                Ok(Stmt::Repeat { cond, body, span: start.to(end) })
            }

            other => Err(Diag::new(
                format!("`{}` is not an action Verb-AL knows", other),
                verb_span,
                "use one of note, say, assign, branch or repetition",
            )),
        }
    }

    /// Statements up to (but not consuming) one of `stops`.
    fn block(&mut self, stops: &[&str], closer: &str, opened_at: Span) -> Result<Vec<Stmt>, Diag> {
        let mut out = Vec::new();
        loop {
            // A comma between statements is permitted but never required.
            while *self.peek() == Tok::Comma {
                self.bump();
            }
            if let Some(w) = self.peek_word() {
                if stops.contains(&w) {
                    return Ok(out);
                }
            }
            if self.at_eof() {
                return Err(Diag::new(
                    format!(
                        "every block closes with `{}`; the program ends while this one is still open",
                        closer
                    ),
                    opened_at,
                    format!("add `{}` where this block should end", closer),
                ));
            }
            out.push(self.statement()?);
        }
    }

    // ---- expressions ------------------------------------------------------

    fn expression(&mut self) -> Result<Expr, Diag> {
        if let Some(op) = self.peek_word().and_then(UnOp::from_word) {
            let op_span = self.bump().span;
            let operand = self.primary()?;
            let span = op_span.to(operand.span());
            let expr = Expr::Unary { op, operand: Box::new(operand), span };
            self.reject_second_operator(&expr)?;
            return Ok(expr);
        }

        let lhs = self.primary()?;
        let Some(op) = self.peek_word().and_then(BinOp::from_word) else {
            return Ok(lhs);
        };
        let op_span = self.bump().span;
        let rhs = self.primary()?;
        let span = lhs.span().to(rhs.span());
        let expr = Expr::Binary { op, lhs: Box::new(lhs), rhs: Box::new(rhs), op_span, span };
        self.reject_second_operator(&expr)?;
        Ok(expr)
    }

    /// Verb-AL has no precedence table, because a precedence table is a fact
    /// left implicit. Two operators in one expression is an error.
    fn reject_second_operator(&self, _already: &Expr) -> Result<(), Diag> {
        let Some(w) = self.peek_word() else { return Ok(()) };
        if BinOp::from_word(w).is_none() && UnOp::from_word(w).is_none() {
            return Ok(());
        }
        Err(Diag::new(
            format!(
                "an expression holds at most one operator, because Verb-AL has no precedence table; `{}` is a second one",
                w
            ),
            self.peek_span(),
            "parenthesise the part that happens first, as in (a times b) plus c",
        ))
    }

    fn primary(&mut self) -> Result<Expr, Diag> {
        match self.peek().clone() {
            Tok::Ident(name) => {
                let span = self.bump().span;
                Ok(Expr::Ident { name, span })
            }
            Tok::Lit(raw) => {
                let span = self.bump().span;
                Ok(Expr::Lit { raw, span })
            }
            Tok::LParen => {
                let open = self.bump().span;
                let inner = self.expression()?;
                let close = self.expect(Tok::RParen, "to close the parenthesised expression")?;
                Ok(match inner {
                    Expr::Ident { name, .. } => Expr::Ident { name, span: open.to(close) },
                    Expr::Lit { raw, .. } => Expr::Lit { raw, span: open.to(close) },
                    Expr::Unary { op, operand, .. } => {
                        Expr::Unary { op, operand, span: open.to(close) }
                    }
                    Expr::Binary { op, lhs, rhs, op_span, .. } => {
                        Expr::Binary { op, lhs, rhs, op_span, span: open.to(close) }
                    }
                })
            }
            other => Err(Diag::new(
                format!(
                    "an operand is a \"name\", a 'value' or a parenthesised expression; {} is none of these",
                    other.describe()
                ),
                self.peek_span(),
                "write a declared name, a literal value, or `(` to open a sub-expression",
            )),
        }
    }
}

/// Turn a spelled-out descriptor into the type it describes.
fn build_type(
    head: &str,
    head_span: Span,
    parts: &[(String, Span)],
    span: Span,
) -> Result<Type, Diag> {
    let words: Vec<&str> = parts.iter().map(|(w, _)| w.as_str()).collect();

    match head {
        "integer" => {
            if words.len() != 3 {
                return Err(Diag::new(
                    format!("an integer descriptor has three parts after `integer`; this has {}", words.len()),
                    span,
                    "write integer.<n>-sign-bit(s).<m>-value-bit(s).twos-complement, \
                     for instance integer.1-sign-bit.31-value-bits.twos-complement",
                ));
            }
            let signs = counted(&parts[0], "sign-bit")?;
            let values = counted(&parts[1], "value-bit")?;
            let signed = match signs {
                0 => false,
                1 => true,
                n => {
                    return Err(Diag::new(
                        format!("a number carries 0 or 1 sign bits; this claims {}", n),
                        parts[0].1,
                        "write `1-sign-bit` for a signed integer, or `0-sign-bits` for an unsigned one",
                    ))
                }
            };
            let encoding = words[2];
            match (signed, encoding) {
                (true, "twos-complement") | (false, "unsigned-binary") => {}
                (true, other) => {
                    return Err(Diag::new(
                        format!("an integer with a sign bit is encoded `twos-complement`; this says `{}`", other),
                        parts[2].1,
                        "write `twos-complement`, or drop the sign bit and write `unsigned-binary`",
                    ))
                }
                (false, other) => {
                    return Err(Diag::new(
                        format!("an integer with no sign bit is encoded `unsigned-binary`; this says `{}`", other),
                        parts[2].1,
                        "write `unsigned-binary`, or add a sign bit and write `twos-complement`",
                    ))
                }
            }
            let width = signs + values;
            if width == 0 || width > 64 {
                return Err(Diag::new(
                    format!("an integer is 1 to 64 bits wide, sign bit included; this asks for {}", width),
                    span,
                    "reduce the value bits so the sign bit and value bits total 64 or fewer",
                ));
            }
            Ok(Type::Int { width, signed })
        }

        "float" => {
            if words.len() != 3 {
                return Err(Diag::new(
                    format!("a float descriptor has three parts after `float`; this has {}", words.len()),
                    span,
                    "write float.1-sign-bit.<e>-exponent-bits.<m>-explicit-mantissa-bits, \
                     for instance float.1-sign-bit.8-exponent-bits.23-explicit-mantissa-bits",
                ));
            }
            let signs = counted(&parts[0], "sign-bit")?;
            if signs != 1 {
                return Err(Diag::new(
                    format!("every float carries exactly one sign bit; this claims {}", signs),
                    parts[0].1,
                    "write `1-sign-bit`",
                ));
            }
            let exponent = counted(&parts[1], "exponent-bit")?;
            let mantissa = counted(&parts[2], "explicit-mantissa-bit")?;
            FloatKind::from_layout(exponent, mantissa).map(Type::Float).ok_or_else(|| {
                Diag::new(
                    format!(
                        "a float layout is one of four; none has {} exponent bits and {} explicit mantissa bits",
                        exponent, mantissa
                    ),
                    span,
                    "use 5/10 (half), 8/7 (bfloat), 8/23 (single) or 11/52 (double) — \
                     the mantissa count excludes the implicit leading one",
                )
            })
        }

        "truth" => {
            if words != ["1-bit"] {
                return Err(Diag::new(
                    "a truth descriptor is exactly `truth.1-bit`",
                    span,
                    "write truth.1-bit",
                ));
            }
            Ok(Type::Truth)
        }

        "character" => {
            if words != ["32-bits", "unicode-scalar"] {
                return Err(Diag::new(
                    "a character descriptor is exactly `character.32-bits.unicode-scalar`",
                    span,
                    "write character.32-bits.unicode-scalar",
                ));
            }
            Ok(Type::Character)
        }

        "text" => {
            if words != ["utf-8", "pointer-and-length"] {
                return Err(Diag::new(
                    "a text descriptor is exactly `text.utf-8.pointer-and-length`",
                    span,
                    "write text.utf-8.pointer-and-length",
                ));
            }
            Ok(Type::Text)
        }

        other => Err(Diag::new(
            format!("`{}` is not a type Verb-AL knows", other),
            head_span,
            "use one of integer, float, truth, character or text",
        )),
    }
}

/// Reads a part like `31-value-bits`, insisting the noun agrees in number.
fn counted(part: &(String, Span), noun: &str) -> Result<u32, Diag> {
    let (word, span) = part;
    let Some((digits, rest)) = word.split_once('-') else {
        return Err(Diag::new(
            format!("each part of this descriptor is a count followed by `{}`; this part has no count", noun),
            *span,
            format!("write a number, a hyphen and `{}`, as in `8-{}s`", noun, noun),
        ));
    };
    let Ok(n) = digits.parse::<u32>() else {
        return Err(Diag::new(
            format!("each part of this descriptor begins with a number; `{}` does not", word),
            *span,
            format!("write a number before the hyphen, as in `8-{}s`", noun),
        ));
    };
    let expected_singular = n == 1;
    let (stem, plural) = match rest.strip_suffix('s') {
        Some(stem) => (stem, true),
        None => (rest, false),
    };
    if stem != noun {
        return Err(Diag::new(
            format!("this part of the descriptor counts {}s; it says `{}` instead", noun, rest),
            *span,
            format!("write `{}-{}s`", n, noun),
        ));
    }
    if plural == expected_singular {
        return Err(Diag::new(
            format!(
                "Verb-AL insists the noun agrees with the number; `{}` does not",
                word
            ),
            *span,
            format!("write `{}`", crate::types::plural(n, noun)),
        ));
    }
    Ok(n)
}
