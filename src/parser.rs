//! Verb-AL's one and only statement grammar: a comma-separated list of
//! `key:value` attributes, terminated by a word.

use crate::ast::*;
use crate::diag::{Diag, Span};
use crate::lexer::{Tok, Token};
use crate::types::{CharClass, FloatKind, Type};

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
                format!("expected {} {}, found {}", tok.describe(), context, self.peek().describe()),
                self.peek_span(),
            ))
        }
    }

    fn expect_word(&mut self, word: &str, context: &str) -> Result<Span, Diag> {
        if self.peek_word() == Some(word) {
            Ok(self.bump().span)
        } else {
            Err(Diag::new(
                format!("expected `{}` {}, found {}", word, context, self.peek().describe()),
                self.peek_span(),
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
                format!("expected {}, found {}", context, other.describe()),
                self.peek_span(),
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
                format!("expected {} in double quotes, found {}", context, other.describe()),
                self.peek_span(),
            )
            .note("names are written in \"double quotes\"; values in 'single quotes'")),
        }
    }

    // ---- statements -------------------------------------------------------

    fn statement(&mut self) -> Result<Stmt, Diag> {
        match self.peek_word() {
            Some("privacy") => self.declaration(),
            Some("action") => self.action(),
            _ => Err(Diag::new(
                format!("expected a statement, found {}", self.peek().describe()),
                self.peek_span(),
            )
            .note("a statement begins with `privacy:` (a declaration) or `action:` (an action)")),
        }
    }

    fn declaration(&mut self) -> Result<Stmt, Diag> {
        let start = self.expect_word("privacy", "to begin a declaration")?;
        self.expect(Tok::Colon, "after `privacy`")?;
        let (word, span) = self.any_word("`local` or `public`")?;
        let privacy = match word.as_str() {
            "local" => Privacy::Local,
            "public" => Privacy::Public,
            _ => {
                return Err(Diag::new(format!("`{}` is not a privacy", word), span)
                    .note("privacy is `local` or `public`"))
            }
        };

        self.expect(Tok::Comma, "after the privacy")?;
        self.expect_word("memory", "as the second attribute of a declaration")?;
        self.expect(Tok::Colon, "after `memory`")?;
        let (word, span) = self.any_word("`static` or `automatic`")?;
        let memory = match word.as_str() {
            "static" => MemoryClass::Static,
            "automatic" => MemoryClass::Automatic,
            _ => {
                return Err(Diag::new(format!("`{}` is not a memory class", word), span)
                    .note("memory is `static` (one cell for the whole run) or `automatic` (a cell per block entry)"))
            }
        };

        self.expect(Tok::Comma, "after the memory class")?;
        self.expect_word("type", "as the third attribute of a declaration")?;
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
        let start = self.expect_word("name", "as the fourth attribute of a declaration")?;
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
                return Err(Diag::new(format!("`{}` is not a character class", w), s).note(
                    "the classes are string, digit, space, comma, period, hyphen, underscore, \
                     apostrophe, exclamation, question, colon, slash, emoji",
                ));
            };
            if classes.contains(&class) {
                return Err(Diag::new(format!("`{}` is permitted twice", w), s)
                    .note("a descriptor is a set of permissions, so each class is listed once"));
            }
            classes.push(class);
        }
        if classes.is_empty() {
            return Err(Diag::new("this name descriptor permits no character classes", span)
                .note("a name permitted nothing could not be written at all"));
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
                            format!("a remark is a 'single quoted' value, found {}", other.describe()),
                            self.peek_span(),
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
                let then = self.block(&["otherwise", "end-branch"], "end-branch")?;
                let mut otherwise = Vec::new();
                if self.peek_word() == Some("otherwise") {
                    self.bump();
                    self.expect(Tok::Colon, "after `otherwise`")?;
                    otherwise = self.block(&["end-branch"], "end-branch")?;
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
                let body = self.block(&["end-repetition"], "end-repetition")?;
                let end = self.expect_word("end-repetition", "to close the repetition")?;
                Ok(Stmt::Repeat { cond, body, span: start.to(end) })
            }

            other => Err(Diag::new(format!("`{}` is not an action Verb-AL knows", other), verb_span)
                .note("the actions are note, say, assign, branch and repetition")),
        }
    }

    /// Statements up to (but not consuming) one of `stops`.
    fn block(&mut self, stops: &[&str], closer: &str) -> Result<Vec<Stmt>, Diag> {
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
                    format!("the program ends before this block is closed with `{}`", closer),
                    self.peek_span(),
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
            format!("`{}` is a second operator, and nothing here says which happens first", w),
            self.peek_span(),
        )
        .note("Verb-AL has no precedence table, because a precedence table is a fact left implicit")
        .note("parenthesise the part that happens first"))
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
                format!("expected a name, a value or `(`, found {}", other.describe()),
                self.peek_span(),
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
                return Err(Diag::new("an integer descriptor has three parts", span).note(
                    "write integer.<n>-sign-bit(s).<m>-value-bit(s).twos-complement|unsigned-binary",
                ));
            }
            let signs = counted(&parts[0], "sign-bit")?;
            let values = counted(&parts[1], "value-bit")?;
            let signed = match signs {
                0 => false,
                1 => true,
                n => {
                    return Err(Diag::new(
                        format!("a number has 0 or 1 sign bits, not {}", n),
                        parts[0].1,
                    ))
                }
            };
            let encoding = words[2];
            match (signed, encoding) {
                (true, "twos-complement") | (false, "unsigned-binary") => {}
                (true, other) => {
                    return Err(Diag::new(
                        format!("a signed integer is encoded `twos-complement`, not `{}`", other),
                        parts[2].1,
                    ))
                }
                (false, other) => {
                    return Err(Diag::new(
                        format!("an unsigned integer is encoded `unsigned-binary`, not `{}`", other),
                        parts[2].1,
                    ))
                }
            }
            let width = signs + values;
            if width == 0 || width > 64 {
                return Err(Diag::new(
                    format!("{} bits is not a width Verb-AL supports", width),
                    span,
                )
                .note("integers are 1 to 64 bits wide, sign bit included"));
            }
            Ok(Type::Int { width, signed })
        }

        "float" => {
            if words.len() != 3 {
                return Err(Diag::new("a float descriptor has three parts", span).note(
                    "write float.1-sign-bit.<e>-exponent-bits.<m>-explicit-mantissa-bits",
                ));
            }
            let signs = counted(&parts[0], "sign-bit")?;
            if signs != 1 {
                return Err(Diag::new(
                    format!("every float has exactly one sign bit, not {}", signs),
                    parts[0].1,
                ));
            }
            let exponent = counted(&parts[1], "exponent-bit")?;
            let mantissa = counted(&parts[2], "explicit-mantissa-bit")?;
            FloatKind::from_layout(exponent, mantissa).map(Type::Float).ok_or_else(|| {
                Diag::new(
                    format!(
                        "no float layout has {} exponent bits and {} explicit mantissa bits",
                        exponent, mantissa
                    ),
                    span,
                )
                .note("the layouts are 5/10 (half), 8/7 (bfloat), 8/23 (single) and 11/52 (double)")
                .note("the mantissa count excludes the implicit leading one")
            })
        }

        "truth" => {
            if words != ["1-bit"] {
                return Err(Diag::new("a truth value is `truth.1-bit`", span));
            }
            Ok(Type::Truth)
        }

        "character" => {
            if words != ["32-bits", "unicode-scalar"] {
                return Err(Diag::new(
                    "a character is `character.32-bits.unicode-scalar`",
                    span,
                ));
            }
            Ok(Type::Character)
        }

        "text" => {
            if words != ["utf-8", "pointer-and-length"] {
                return Err(Diag::new("text is `text.utf-8.pointer-and-length`", span));
            }
            Ok(Type::Text)
        }

        other => Err(Diag::new(format!("`{}` is not a type Verb-AL knows", other), head_span)
            .note("the types are integer, float, truth, character and text")),
    }
}

/// Reads a part like `31-value-bits`, insisting the noun agrees in number.
fn counted(part: &(String, Span), noun: &str) -> Result<u32, Diag> {
    let (word, span) = part;
    let Some((digits, rest)) = word.split_once('-') else {
        return Err(Diag::new(format!("expected a count of {}s here", noun), *span));
    };
    let Ok(n) = digits.parse::<u32>() else {
        return Err(Diag::new(format!("`{}` does not begin with a number", word), *span));
    };
    let expected_singular = n == 1;
    let (stem, plural) = match rest.strip_suffix('s') {
        Some(stem) => (stem, true),
        None => (rest, false),
    };
    if stem != noun {
        return Err(Diag::new(format!("expected {}s here, found `{}`", noun, rest), *span));
    }
    if plural == expected_singular {
        return Err(Diag::new(
            format!(
                "write `{}`, not `{}` — Verb-AL insists the noun agrees with the number",
                crate::types::plural(n, noun),
                word
            ),
            *span,
        ));
    }
    Ok(n)
}
