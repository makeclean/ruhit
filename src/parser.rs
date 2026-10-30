//! Recursive-descent parser for HIT with line-oriented error recovery.
//!
//! The parser never panics on malformed input. Instead it records a
//! [`Diagnostic`] and synchronizes at the next newline, block boundary or
//! closing bracket. This is important for an editor: a partially typed file
//! should still produce a useful AST.

use std::collections::HashMap;

use crate::ast::{
    split_header, ArrayElement, Block, Comment, Diagnostic, Document, Item, Parameter, QuoteMark,
    Severity, Span, Value, ValueKind,
};
use crate::lexer::{tokenize_with, LexOptions, Token, TokenKind};

/// Hard limit for recursive block/array nesting. This keeps pathological input
/// from overflowing the native or WASM stack.
const MAX_NESTING: usize = 128;

/// Parser behaviour switches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ParseOptions {
    /// Enable WASP HIT compatibility. See [`LexOptions::wasp_compat`].
    pub wasp_compat: bool,
}

impl ParseOptions {
    pub const fn wasp() -> Self {
        Self { wasp_compat: true }
    }
}

/// Parse HIT source into an AST. Recoverable errors are stored in
/// [`Document::diagnostics`].
pub fn parse(source: &str) -> Document {
    parse_with_options(source, ParseOptions::default())
}

/// Parse HIT source with explicit options.
pub fn parse_with_options(source: &str, options: ParseOptions) -> Document {
    let lexed = tokenize_with(
        source,
        LexOptions {
            wasp_compat: options.wasp_compat,
        },
    );
    let mut parser = Parser {
        source,
        tokens: lexed.tokens,
        pos: 0,
        diagnostics: lexed.diagnostics,
        array_depth: 0,
        wasp_compat: options.wasp_compat,
        wasp_recovery: None,
    };
    parser.parse_document()
}

/// Parse HIT source and return only its diagnostics. This is the native
/// equivalent of the WASM `validate_hit` binding.
pub fn validate(source: &str) -> Vec<Diagnostic> {
    parse(source).diagnostics
}

struct Parser<'a> {
    source: &'a str,
    tokens: Vec<Token>,
    pos: usize,
    diagnostics: Vec<Diagnostic>,
    array_depth: usize,
    wasp_compat: bool,
    wasp_recovery: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HeaderKind {
    Open,
    Close,
    Invalid,
}

#[derive(Debug, Clone)]
struct HeaderInfo {
    raw: String,
    span: Span,
    kind: HeaderKind,
    /// True when the closing `]` was missing, so WASP already reported a
    /// header error. The block still exists, but an empty block should not
    /// produce a second end-of-file diagnostic.
    unterminated: bool,
}

impl<'a> Parser<'a> {
    fn parse_document(&mut self) -> Document {
        let start_position = self
            .tokens
            .first()
            .map(|token| token.span.start)
            .unwrap_or_default();

        let mut items = Vec::new();
        let mut last_line = 0;

        loop {
            self.skip_newlines();
            let token = self.peek().clone();
            match token.kind {
                TokenKind::Eof => break,
                TokenKind::LBracket => {
                    if self.is_close_header_ahead() {
                        self.diagnostics.push(Diagnostic::error(
                            "unexpected-closing-block",
                            "unexpected closing block header at top level",
                            token.span,
                        ));
                        let _ = self.parse_header();
                        continue;
                    }
                    if let Some(block) = self.parse_block(0) {
                        last_line = block.span.end.line;
                        items.push(Item::Block(block));
                    }
                }
                TokenKind::Comment(_) => {
                    let comment = self.parse_comment(last_line);
                    last_line = comment.span.end.line;
                    items.push(Item::Comment(comment));
                }
                TokenKind::Word(_) | TokenKind::String(_) => {
                    if let Some(item) = self.parse_parameter() {
                        last_line = item.span().end.line;
                        items.push(item);
                    }
                }
                TokenKind::RBracket => {
                    self.diagnostics.push(Diagnostic::error(
                        "unexpected-token",
                        "unexpected ']' outside of a block",
                        token.span,
                    ));
                    self.advance();
                }
                TokenKind::Equals
                | TokenKind::OverrideEquals
                | TokenKind::Comma
                | TokenKind::Semicolon
                | TokenKind::Quote(_) => {
                    self.diagnostics.push(Diagnostic::error(
                        "unexpected-token",
                        format!("unexpected {}", token.kind.describe()),
                        token.span,
                    ));
                    self.advance();
                }
                TokenKind::Newline => {
                    self.advance();
                }
            }
        }

        self.lint_duplicate_parameters(&items, None);

        let span = match (items.first(), items.last()) {
            (Some(first), Some(last)) => Span::new(first.span().start, last.span().end),
            _ => Span::point(start_position),
        };

        if !self.wasp_compat {
            self.diagnostics.sort_by_key(|diagnostic| {
                (
                    diagnostic.span.start.offset,
                    diagnostic.span.end.offset,
                    diagnostic.code.clone(),
                )
            });
        }

        Document::new(items, span, std::mem::take(&mut self.diagnostics))
    }

    fn parse_block(&mut self, depth: usize) -> Option<Block> {
        let header = self.parse_header();
        if header.kind != HeaderKind::Open {
            return None;
        }

        if depth >= MAX_NESTING {
            self.diagnostics.push(Diagnostic::error(
                "nesting-too-deep",
                format!("maximum block nesting depth of {MAX_NESTING} exceeded"),
                header.span,
            ));
            self.skip_block_contents();
            return None;
        }

        let (name, _) = split_header(&header.raw);
        let name = if name.is_empty() {
            header.raw.clone()
        } else {
            name
        };

        let mut items = Vec::new();
        let mut last_line = header.span.end.line;
        let mut end = header.span.end;
        let mut closing = None;
        let mut close_span = None;

        loop {
            self.skip_newlines();
            let token = self.peek().clone();
            match token.kind {
                TokenKind::Eof => {
                    if self.wasp_compat {
                        if items.is_empty() {
                            if !header.unterminated {
                                self.diagnostics.push(Diagnostic::error(
                                    "syntax-error",
                                    "syntax error, unexpected end of file",
                                    Span::point(token.span.start),
                                ));
                            }
                        } else {
                            self.diagnostics.push(Diagnostic::error(
                                "syntax-error",
                                "syntax error, unexpected end of file, expecting block terminator",
                                Span::point(header.span.start),
                            ));
                        }
                    } else {
                        self.diagnostics.push(Diagnostic::error(
                            "unclosed-block",
                            format!("unclosed block '{name}'; expected a closing header"),
                            header.span,
                        ));
                    }
                    break;
                }
                TokenKind::LBracket => {
                    if self.is_close_header_ahead() {
                        let close = self.parse_header();
                        closing = Some(close.raw);
                        close_span = Some(close.span);
                        end = close.span.end;
                        break;
                    }
                    if let Some(block) = self.parse_block(depth + 1) {
                        last_line = block.span.end.line;
                        end = block.span.end;
                        items.push(Item::Block(block));
                    }
                }
                TokenKind::Comment(_) => {
                    let comment = self.parse_comment(last_line);
                    last_line = comment.span.end.line;
                    end = comment.span.end;
                    items.push(Item::Comment(comment));
                }
                TokenKind::Word(_) | TokenKind::String(_) => {
                    if let Some(item) = self.parse_parameter() {
                        last_line = item.span().end.line;
                        end = item.span().end;
                        items.push(item);
                    }
                }
                TokenKind::RBracket => {
                    self.diagnostics.push(Diagnostic::error(
                        "unexpected-token",
                        "unexpected ']' without a matching '['",
                        token.span,
                    ));
                    self.advance();
                }
                TokenKind::Equals
                | TokenKind::OverrideEquals
                | TokenKind::Comma
                | TokenKind::Semicolon
                | TokenKind::Quote(_) => {
                    self.diagnostics.push(Diagnostic::error(
                        "unexpected-token",
                        format!("unexpected {}", token.kind.describe()),
                        token.span,
                    ));
                    self.advance();
                }
                TokenKind::Newline => {
                    self.advance();
                }
            }
        }

        self.lint_duplicate_parameters(&items, Some(&name));

        let span = Span::new(header.span.start, end);
        Some(Block::new(
            name,
            header.raw,
            items,
            span,
            header.span,
            closing,
            close_span,
        ))
    }

    fn wasp_unexpected(&mut self, token: &Token, expecting: Option<&str>) {
        if self.wasp_recovery.is_some() {
            return;
        }
        let position = if matches!(token.kind, TokenKind::Newline) {
            token.span.end
        } else {
            token.span.start
        };
        let message = match expecting {
            Some(expected) => format!(
                "syntax error, unexpected {}, expecting {}",
                wasp_token_display(&token.kind),
                expected
            ),
            None => format!(
                "syntax error, unexpected {}",
                wasp_token_display(&token.kind)
            ),
        };
        self.diagnostics.push(Diagnostic::error(
            "syntax-error",
            message,
            Span::point(position),
        ));
        self.wasp_recovery = Some(0);
    }

    fn parse_header(&mut self) -> HeaderInfo {
        let open = self.advance();
        let open_span = open.span;

        if matches!(self.peek().kind, TokenKind::RBracket) {
            let close = self.advance();
            return HeaderInfo {
                raw: String::new(),
                span: Span::new(open_span.start, close.span.end),
                kind: HeaderKind::Close,
                unterminated: false,
            };
        }

        if matches!(
            self.peek().kind,
            TokenKind::Newline | TokenKind::Eof | TokenKind::Comment(_)
        ) {
            let token = self.peek().clone();
            let end = if matches!(token.kind, TokenKind::Newline) {
                token.span.end
            } else {
                token.span.start
            };
            if self.wasp_compat {
                self.wasp_unexpected(&token, Some("object name"));
                return HeaderInfo {
                    raw: String::new(),
                    span: Span::new(open_span.start, end),
                    kind: HeaderKind::Open,
                    unterminated: true,
                };
            }
            self.diagnostics.push(Diagnostic::error(
                "unclosed-block-header",
                "expected ']' after block header",
                Span::new(open_span.start, end),
            ));
            return HeaderInfo {
                raw: String::new(),
                span: Span::new(open_span.start, end),
                kind: HeaderKind::Invalid,
                unterminated: false,
            };
        }

        let content_start = open_span.end.offset as usize;

        loop {
            match self.peek().kind {
                TokenKind::RBracket => {
                    let close = self.advance();
                    let raw = self.source
                        [content_start..close.span.start.offset as usize]
                        .trim()
                        .to_string();
                    let kind = if is_close_raw(&raw) {
                        HeaderKind::Close
                    } else if raw.is_empty() {
                        HeaderKind::Invalid
                    } else {
                        HeaderKind::Open
                    };
                    return HeaderInfo {
                        raw,
                        span: Span::new(open_span.start, close.span.end),
                        kind,
                        unterminated: false,
                    };
                }
                TokenKind::Newline | TokenKind::Eof | TokenKind::Comment(_) => {
                    let token = self.peek().clone();
                    let end = if matches!(token.kind, TokenKind::Newline) {
                        token.span.end
                    } else {
                        token.span.start
                    };
                    let raw = self.source[content_start..end.offset as usize]
                        .trim()
                        .to_string();
                    if self.wasp_compat {
                        self.wasp_unexpected(&token, Some("]"));
                    } else {
                        self.diagnostics.push(Diagnostic::error(
                            "unclosed-block-header",
                            "expected ']' to close block header",
                            Span::new(open_span.start, end),
                        ));
                    }
                    let kind = if raw.is_empty() || is_close_raw(&raw) {
                        HeaderKind::Invalid
                    } else {
                        HeaderKind::Open
                    };
                    return HeaderInfo {
                        raw,
                        span: Span::new(open_span.start, end),
                        kind,
                        unterminated: true,
                    };
                }
                TokenKind::LBracket => {
                    let nested = self.advance();
                    self.diagnostics.push(Diagnostic::error(
                        "unexpected-token",
                        "unexpected '[' inside block header",
                        nested.span,
                    ));
                }
                _ => {
                    self.advance();
                }
            }
        }
    }

    fn parse_parameter(&mut self) -> Option<Item> {
        let key_token = self.advance();
        let key = match &key_token.kind {
            TokenKind::Word(word) => word.clone(),
            TokenKind::String(literal) => literal.value.clone(),
            _ => return None,
        };

        if !matches!(self.peek().kind, TokenKind::Equals | TokenKind::OverrideEquals) {
            if self.wasp_compat {
                let token = self.peek().clone();
                self.wasp_unexpected(&token, Some("= or :="));
                let mut parameter = Parameter::new(
                    key,
                    Value::missing(Span::point(key_token.span.start)),
                    key_token.span,
                    key_token.span,
                    Span::point(key_token.span.end),
                );
                parameter.assign = String::new();
                return Some(Item::Parameter(parameter));
            }
            self.diagnostics.push(Diagnostic::error(
                "missing-equals",
                format!("expected '=' after parameter name '{key}'"),
                key_token.span,
            ));
            self.recover_to_line_end();
            return None;
        }

        let equals = self.advance();
        let assign = self.source
            [equals.span.start.offset as usize..equals.span.end.offset as usize]
            .to_string();
        let value = self.parse_value_sequence(equals.span, &key);
        let value_end = if value.is_missing() {
            equals.span.end
        } else {
            value.span.end
        };
        let span = Span::new(key_token.span.start, value_end);
        let value_span = if value.is_missing() {
            Span::point(equals.span.end)
        } else {
            value.span
        };

        let mut parameter = Parameter::new(key, value, span, key_token.span, value_span);
        parameter.override_assign = matches!(equals.kind, TokenKind::OverrideEquals);
        parameter.assign = assign;
        Some(Item::Parameter(parameter))
    }

    /// Report a missing parameter value the way WASP's bison grammar does:
    /// first the generic unexpected-token error, then the parameter-specific
    /// "missing or malformed value" error at the assignment.
    fn report_missing_value(&mut self, equals: Span, key: &str) -> Value {
        let token = self.peek().clone();
        if self.wasp_compat {
            let position = if matches!(token.kind, TokenKind::Newline) {
                // WASP's EOL token is located at the start of the next line.
                token.span.end
            } else {
                token.span.start
            };
            if self.wasp_recovery.is_none() {
                self.diagnostics.push(Diagnostic::error(
                    "syntax-error",
                    format!(
                        "syntax error, unexpected {}",
                        wasp_token_display(&token.kind)
                    ),
                    Span::point(position),
                ));
                self.wasp_recovery = Some(0);
            }
            self.diagnostics.push(Diagnostic::error(
                "missing-value",
                format!("syntax error, '{key}' has a missing or malformed value"),
                Span::point(equals.start),
            ));
        } else {
            self.diagnostics.push(Diagnostic::error(
                "missing-value",
                "expected a value after '='",
                Span::point(token.span.start),
            ));
        }
        Value::missing(Span::point(equals.start))
    }

    fn parse_value_sequence(&mut self, equals: Span, key: &str) -> Value {
        let start = self.peek().span.start;
        let mut multiline = false;

        // Allow a value to begin on the next line, but only when the next
        // non-blank token really is a value. Otherwise the newline belongs to
        // the missing-value error and the following parameter.
        if matches!(self.peek().kind, TokenKind::Newline) {
            let saved = self.pos;
            let skipped = self.skip_newlines();
            if skipped > 0
                && self.peek_starts_value()
                && !self.assignment_ahead()
                && !self.is_close_header_ahead()
            {
                multiline = true;
            } else {
                self.pos = saved;
            }
        }

        if self.is_close_header_ahead() {
            return self.report_missing_value(equals, key);
        }

        if self.wasp_compat {
            if !self.peek_starts_value() {
                return self.report_missing_value(equals, key);
            }
            return self.parse_wasp_value_sequence(start);
        }

        let first = self.parse_single_value();
        if first.is_missing() {
            return first;
        }

        let mut elements = vec![ArrayElement::Value(first)];
        let mut had_comma = false;
        let mut trailing_comma;

        loop {
            let last_end_line = elements
                .last()
                .map(ArrayElement::span)
                .map(|span| span.end.line)
                .unwrap_or(equals.end.line);

            if matches!(self.peek().kind, TokenKind::Comma) {
                self.advance();
                had_comma = true;
                trailing_comma = true;
                if self.skip_newlines() > 0 {
                    multiline = true;
                }
                if self.is_value_terminator_after_comma(last_end_line) {
                    break;
                }
                let value = self.parse_single_value();
                if value.is_missing() {
                    break;
                }
                elements.push(ArrayElement::Value(value));
                continue;
            }

            trailing_comma = false;

            // Unbracketed vectors may be space separated: `values = 1 2 3`.
            if self.peek_starts_value()
                && !self.assignment_ahead()
                && !(matches!(self.peek().kind, TokenKind::LBracket)
                    && self.is_close_header_ahead())
                && self.peek().span.start.line == last_end_line
            {
                let value = self.parse_single_value();
                if value.is_missing() {
                    break;
                }
                elements.push(ArrayElement::Value(value));
                continue;
            }

            break;
        }

        if elements.len() == 1 && !had_comma {
            match elements.pop() {
                Some(ArrayElement::Value(value)) => value,
                _ => Value::missing(Span::point(start)),
            }
        } else {
            let end = elements
                .last()
                .map(ArrayElement::span)
                .map(|span| span.end)
                .unwrap_or(start);
            Value::array(
                elements,
                multiline,
                trailing_comma,
                Span::new(start, end),
            )
        }
    }

    fn parse_single_value(&mut self) -> Value {
        let token = self.peek().clone();
        if self.array_depth == 0
            && matches!(token.kind, TokenKind::LBracket)
            && self.is_close_header_ahead()
        {
            // `[]` belongs to the enclosing block, not to the value.
            return Value::missing(Span::point(token.span.start));
        }
        match token.kind {
            TokenKind::String(literal) => {
                self.advance();
                Value::quoted_string(literal.value, literal.raw, literal.quote, token.span)
            }
            TokenKind::Word(word) => {
                self.advance();
                classify_word(word, token.span)
            }
            TokenKind::LBracket => self.parse_bracket_array(),
            _ => {
                self.diagnostics.push(Diagnostic::error(
                    "missing-value",
                    format!("expected a value, found {}", token.kind.describe()),
                    token.span,
                ));
                if matches!(token.kind, TokenKind::Comma) {
                    self.advance();
                }
                Value::missing(Span::point(token.span.start))
            }
        }
    }

    fn parse_wasp_value_sequence(&mut self, start: crate::ast::Position) -> Value {
        match self.peek().kind {
            TokenKind::Quote(_) => self.parse_quoted_array(),
            TokenKind::String(_) if self.peek_quote() == Some('"') => {
                self.parse_double_quoted_sequence(start)
            }
            _ => self.parse_wasp_single_value(),
        }
    }

    fn peek_quote(&self) -> Option<char> {
        match &self.peek().kind {
            TokenKind::String(literal) => Some(literal.quote),
            _ => None,
        }
    }

    /// WASP treats consecutive double-quoted strings as one array.
    fn parse_double_quoted_sequence(&mut self, start: crate::ast::Position) -> Value {
        let mut elements = Vec::new();
        loop {
            match self.peek().kind {
                TokenKind::String(_) if self.peek_quote() == Some('"') => {
                    let token = self.advance();
                    if let TokenKind::String(literal) = token.kind {
                        elements.push(ArrayElement::Value(Value::quoted_string(
                            literal.value,
                            literal.raw,
                            literal.quote,
                            token.span,
                        )));
                    }
                }
                TokenKind::Newline => {
                    // WASP's lexer gobbles newlines between adjacent quoted
                    // strings.
                    self.advance();
                }
                _ => break,
            }
        }

        if elements.len() == 1 {
            match elements.pop() {
                Some(ArrayElement::Value(value)) => value,
                _ => Value::missing(Span::point(start)),
            }
        } else {
            let end = elements
                .last()
                .map(ArrayElement::span)
                .map(|span| span.end)
                .unwrap_or(start);
            Value::array(elements, false, false, Span::new(start, end))
        }
    }

    /// Parse `'a b;c'` and adjacent groups (`'a' 'b'`) as one array.
    fn parse_quoted_array(&mut self) -> Value {
        let open = self.advance();
        let start = open.span.start;
        let mut elements = Vec::new();
        let mut end = open.span.end;
        elements.push(ArrayElement::Quote(QuoteMark::new('\'', open.span)));
        let mut expect_open = false;

        loop {
            if expect_open {
                match self.peek().kind.clone() {
                    TokenKind::Quote(_) => {
                        let token = self.advance();
                        end = token.span.end;
                        elements.push(ArrayElement::Quote(QuoteMark::new('\'', token.span)));
                        expect_open = false;
                        continue;
                    }
                    _ => break,
                }
            }

            match self.peek().kind.clone() {
                TokenKind::Quote(_) => {
                    let token = self.advance();
                    end = token.span.end;
                    elements.push(ArrayElement::Quote(QuoteMark::new('\'', token.span)));
                    // Adjacent quoted groups concatenate, even when separated
                    // by newlines (WASP's lexer gobbles newlines between
                    // groups).
                    let saved = self.pos;
                    self.skip_newlines();
                    if matches!(self.peek().kind, TokenKind::Quote(_)) {
                        expect_open = true;
                        continue;
                    }
                    self.pos = saved;
                    break;
                }
                TokenKind::Semicolon => {
                    let token = self.advance();
                    end = token.span.end;
                    elements.push(ArrayElement::Semicolon(token.span));
                }
                TokenKind::String(literal) => {
                    let token = self.advance();
                    end = token.span.end;
                    elements.push(ArrayElement::Value(Value::quoted_string(
                        literal.value,
                        literal.raw,
                        literal.quote,
                        token.span,
                    )));
                }
                TokenKind::Word(_) => {
                    let value = self.parse_wasp_array_word();
                    end = value.span.end;
                    elements.push(ArrayElement::Value(value));
                }
                TokenKind::Equals
                | TokenKind::Comma
                | TokenKind::OverrideEquals => {
                    let value = self.parse_wasp_array_word();
                    end = value.span.end;
                    elements.push(ArrayElement::Value(value));
                }
                TokenKind::Newline => {
                    // Newlines inside a WASP array are not tokens to the
                    // parser; consume them transparently.
                    self.advance();
                }
                TokenKind::Eof => {
                    self.diagnostics.push(Diagnostic::error(
                        "unclosed-array",
                        "unclosed quoted array; expected a closing quote",
                        Span::new(start, end),
                    ));
                    break;
                }
                _ => {
                    self.diagnostics.push(Diagnostic::error(
                        "unexpected-token",
                        format!(
                            "unexpected {} inside a quoted array",
                            self.peek().kind.describe()
                        ),
                        self.peek().span,
                    ));
                    self.advance();
                }
            }
        }

        Value::quoted_array(elements, Span::new(start, end))
    }

    /// Merge adjacent lexer tokens (`path/a`, `=`, `1`) back into one WASP
    /// array-string value.
    fn parse_wasp_array_word(&mut self) -> Value {
        let first = self.advance();
        let start = first.span.start;
        let mut end = first.span.end;

        while self.peek().span.start.offset == end.offset
            && matches!(
                self.peek().kind,
                TokenKind::Word(_)
                    | TokenKind::Equals
                    | TokenKind::Comma
                    | TokenKind::OverrideEquals
            )
        {
            end = self.advance().span.end;
        }

        let raw = self.source[start.offset as usize..end.offset as usize].to_string();
        classify_word(raw, Span::new(start, end))
    }

    /// Parse a single WASP value. Unlike the lenient mode, adjacent words are
    /// not collected into a vector: WASP requires quotes for vectors.
    fn parse_wasp_single_value(&mut self) -> Value {
        let token = self.peek().clone();
        match token.kind {
            TokenKind::String(literal) => {
                self.advance();
                Value::quoted_string(literal.value, literal.raw, literal.quote, token.span)
            }
            TokenKind::Word(_)
            | TokenKind::Equals
            | TokenKind::Comma
            | TokenKind::Semicolon => self.parse_wasp_word_value(),
            TokenKind::LBracket => self.parse_bracket_array(),
            _ => {
                self.diagnostics.push(Diagnostic::error(
                    "missing-value",
                    format!("expected a value, found {}", token.kind.describe()),
                    token.span,
                ));
                Value::missing(Span::point(token.span.start))
            }
        }
    }

    /// Reconstruct one WASP `VALUE_STRING`. WASP's value token starts with a
    /// non-quote character and then consumes everything up to whitespace,
    /// `[`, `]` or `#` -- including embedded quotes, `=`, `;` and commas.
    /// `${...}` expressions may contain whitespace.
    fn parse_wasp_word_value(&mut self) -> Value {
        let start = self.peek().span.start;
        let mut offset = start.offset as usize;
        let bytes = self.source.as_bytes();

        while offset < bytes.len() {
            let ch = self.source[offset..].chars().next().unwrap_or('\0');
            if ch == '$' && self.source[offset..].starts_with("${") {
                offset += 2;
                let mut depth = 1usize;
                while offset < bytes.len() {
                    let inner = self.source[offset..].chars().next().unwrap_or('\0');
                    offset += inner.len_utf8();
                    match inner {
                        '{' => depth += 1,
                        '}' => {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                        _ => {}
                    }
                }
                continue;
            }
            if ch.is_whitespace() || matches!(ch, '[' | ']' | '#') {
                break;
            }
            offset += ch.len_utf8();
        }

        let target = offset as u32;
        let mut end = self.peek().span.end;
        while self.peek().span.start.offset < target {
            end = self.advance().span.end;
        }
        let raw = self.source[start.offset as usize..end.offset as usize].to_string();
        classify_word(raw, Span::new(start, end))
    }

    fn parse_bracket_array(&mut self) -> Value {
        let open = self.advance();

        if self.array_depth >= MAX_NESTING {
            self.diagnostics.push(Diagnostic::error(
                "nesting-too-deep",
                format!("maximum array nesting depth of {MAX_NESTING} exceeded"),
                open.span,
            ));
            self.skip_array_contents();
            return Value::missing(open.span);
        }
        self.array_depth += 1;

        let mut elements = Vec::new();
        let mut multiline = false;
        let mut trailing_comma = false;
        let mut last_line = open.span.end.line;
        let mut end = open.span.end;

        loop {
            if self.skip_newlines() > 0 {
                multiline = true;
            }

            while matches!(self.peek().kind, TokenKind::Comment(_)) {
                let comment = self.parse_comment(last_line);
                last_line = comment.span.end.line;
                end = comment.span.end;
                elements.push(ArrayElement::Comment(comment));
                if self.skip_newlines() > 0 {
                    multiline = true;
                }
            }

            match self.peek().kind {
                TokenKind::RBracket => {
                    let close = self.advance();
                    end = close.span.end;
                    break;
                }
                TokenKind::Eof => {
                    self.diagnostics.push(Diagnostic::error(
                        "unclosed-array",
                        "unclosed array; expected ']'",
                        Span::new(open.span.start, end),
                    ));
                    break;
                }
                TokenKind::LBracket
                    if self.is_close_header_ahead()
                        && self.peek().span.start.line > open.span.start.line
                        && !self.closing_header_is_empty_array_element() =>
                {
                    // Most likely the array's ']' is missing and this is the
                    // block's closing header.
                    self.diagnostics.push(Diagnostic::error(
                        "unclosed-array",
                        "unclosed array; expected ']' before the block ended",
                        Span::new(open.span.start, end),
                    ));
                    break;
                }
                TokenKind::Comma => {
                    let comma = self.advance();
                    trailing_comma = true;
                    self.diagnostics.push(Diagnostic::error(
                        "unexpected-token",
                        "unexpected ',' in array; expected a value",
                        comma.span,
                    ));
                }
                TokenKind::Equals => {
                    let equals = self.advance();
                    self.diagnostics.push(Diagnostic::error(
                        "unexpected-token",
                        "unexpected '=' inside an array",
                        equals.span,
                    ));
                }
                TokenKind::Word(_) | TokenKind::String(_) if self.assignment_ahead() => {
                    self.diagnostics.push(Diagnostic::error(
                        "unclosed-array",
                        "unclosed array; expected ']' before the next parameter",
                        Span::new(open.span.start, end),
                    ));
                    break;
                }
                _ => {
                    let value = self.parse_single_value();
                    if value.is_missing() {
                        break;
                    }
                    last_line = value.span.end.line;
                    end = value.span.end;
                    elements.push(ArrayElement::Value(value));

                    if matches!(self.peek().kind, TokenKind::Comma) {
                        self.advance();
                        trailing_comma = true;
                    } else {
                        trailing_comma = false;
                    }
                }
            }
        }

        let value = Value::array(
            elements,
            multiline,
            trailing_comma,
            Span::new(open.span.start, end),
        );
        self.array_depth -= 1;
        value
    }

    fn parse_comment(&mut self, last_line: u32) -> Comment {
        let token = self.advance();
        let text = match token.kind {
            TokenKind::Comment(text) => text,
            _ => String::new(),
        };
        let inline = last_line > 0 && token.span.start.line == last_line;
        Comment::new(text, inline, token.span)
    }

    fn lint_duplicate_parameters(&mut self, items: &[Item], block_name: Option<&str>) {
        let mut seen: HashMap<String, Span> = HashMap::new();
        for item in items {
            let Item::Parameter(parameter) = item else {
                continue;
            };
            if seen.insert(parameter.key.clone(), parameter.key_span).is_some() {
                let location = block_name
                    .map(|name| format!(" in block '{name}'"))
                    .unwrap_or_default();
                self.diagnostics.push(Diagnostic::new(
                    Severity::Warning,
                    "duplicate-parameter",
                    format!(
                        "duplicate parameter '{}'{}; later assignments override earlier ones",
                        parameter.key, location
                    ),
                    parameter.key_span,
                ));
            }
        }
    }

    fn peek(&self) -> &Token {
        self.tokens
            .get(self.pos)
            .unwrap_or_else(|| self.tokens.last().expect("lexer always emits EOF"))
    }

    fn peek_n(&self, n: usize) -> &Token {
        self.tokens
            .get(self.pos + n)
            .unwrap_or_else(|| self.tokens.last().expect("lexer always emits EOF"))
    }

    fn advance(&mut self) -> Token {
        let token = self.peek().clone();
        if !matches!(token.kind, TokenKind::Eof) {
            self.pos += 1;
        }
        if self.wasp_compat {
            if let Some(count) = self.wasp_recovery {
                // Bison suppresses further syntax-error messages until three
                // tokens have been shifted after an error. Newlines are
                // discarded during recovery and do not count.
                if !matches!(token.kind, TokenKind::Newline) {
                    if count + 1 >= 3 {
                        self.wasp_recovery = None;
                    } else {
                        self.wasp_recovery = Some(count + 1);
                    }
                }
            }
        }
        token
    }

    fn skip_newlines(&mut self) -> usize {
        let mut count = 0;
        while matches!(self.peek().kind, TokenKind::Newline) {
            self.advance();
            count += 1;
        }
        count
    }

    fn is_close_header_ahead(&self) -> bool {
        self.is_close_header_at(0)
    }

    fn is_close_header_at(&self, index: usize) -> bool {
        if !matches!(self.peek_n(index).kind, TokenKind::LBracket) {
            return false;
        }
        match &self.peek_n(index + 1).kind {
            TokenKind::RBracket => true,
            TokenKind::Word(word) => {
                is_close_raw(word) && matches!(self.peek_n(index + 2).kind, TokenKind::RBracket)
            }
            _ => false,
        }
    }

    /// Distinguishes an empty array element (`[]`) from a block's closing
    /// header when it appears on its own line inside a multiline array.
    ///
    /// Only a bare `[]` is ambiguous; `[../]` is always a closing header.
    /// A `[]` is treated as an element when, after any newlines and comments,
    /// it is followed by something that continues the enclosing array: another
    /// `]`, a comma, or a value that is not an assignment.
    fn closing_header_is_empty_array_element(&self) -> bool {
        if !matches!(self.peek().kind, TokenKind::LBracket)
            || !matches!(self.peek_n(1).kind, TokenKind::RBracket)
        {
            return false;
        }

        let mut index = 2;
        loop {
            match &self.peek_n(index).kind {
                TokenKind::Newline | TokenKind::Comment(_) => index += 1,
                _ => break,
            }
        }

        match &self.peek_n(index).kind {
            TokenKind::RBracket | TokenKind::Comma => true,
            TokenKind::Word(_) | TokenKind::String(_) => {
                !matches!(self.peek_n(index + 1).kind, TokenKind::Equals)
            }
            // Another bracketed element follows, so this `[]` is an element
            // too. If the following bracket is actually a closing header the
            // enclosing array is unclosed, which is reported on the next
            // iteration.
            TokenKind::LBracket => true,
            _ => false,
        }
    }

    fn assignment_ahead(&self) -> bool {
        matches!(
            self.peek().kind,
            TokenKind::Word(_) | TokenKind::String(_)
        ) && matches!(self.peek_n(1).kind, TokenKind::Equals)
    }

    fn peek_starts_value(&self) -> bool {
        matches!(
            self.peek().kind,
            TokenKind::Word(_)
                | TokenKind::String(_)
                | TokenKind::LBracket
                | TokenKind::Quote(_)
                | TokenKind::Equals
                | TokenKind::Comma
                | TokenKind::Semicolon
                | TokenKind::OverrideEquals
        )
    }

    fn is_value_terminator_after_comma(&self, last_end_line: u32) -> bool {
        match &self.peek().kind {
            TokenKind::Eof
            | TokenKind::Newline
            | TokenKind::Comment(_)
            | TokenKind::RBracket => true,
            // `[]` on the same line as the previous value is an empty array
            // element; on a later line it is the block's closing header.
            TokenKind::LBracket => {
                self.is_close_header_ahead() && self.peek().span.start.line > last_end_line
            },
            TokenKind::Word(_) | TokenKind::String(_) => self.assignment_ahead(),
            TokenKind::Equals
            | TokenKind::OverrideEquals
            | TokenKind::Comma
            | TokenKind::Semicolon
            | TokenKind::Quote(_) => false,
        }
    }

    fn skip_block_contents(&mut self) {
        let mut level = 1usize;
        loop {
            self.skip_newlines();
            match self.peek().kind {
                TokenKind::Eof => break,
                TokenKind::LBracket => {
                    let closes = self.is_close_header_ahead();
                    let _ = self.parse_header();
                    if closes {
                        level = level.saturating_sub(1);
                        if level == 0 {
                            break;
                        }
                    } else {
                        level += 1;
                    }
                }
                _ => {
                    self.advance();
                }
            }
        }
    }

    fn skip_array_contents(&mut self) {
        let mut level = 1usize;
        loop {
            match self.peek().kind {
                TokenKind::Eof => break,
                TokenKind::LBracket => {
                    self.advance();
                    level += 1;
                }
                TokenKind::RBracket => {
                    self.advance();
                    level = level.saturating_sub(1);
                    if level == 0 {
                        break;
                    }
                }
                _ => {
                    self.advance();
                }
            }
        }
    }

    fn recover_to_line_end(&mut self) {
        loop {
            match self.peek().kind {
                TokenKind::Newline => {
                    self.advance();
                    break;
                }
                TokenKind::Eof | TokenKind::RBracket | TokenKind::LBracket => break,
                _ => {
                    self.advance();
                }
            }
        }
    }
}

fn is_close_raw(raw: &str) -> bool {
    matches!(raw.trim(), "" | "." | ".." | "./" | "../")
}

/// The token spelling bison uses in `syntax error, unexpected ...` messages.
fn wasp_token_display(kind: &TokenKind) -> &'static str {
    match kind {
        TokenKind::Eof => "end of file",
        TokenKind::Newline => "end of line",
        TokenKind::Comment(_) => "comment",
        TokenKind::LBracket => "[",
        TokenKind::RBracket => "]",
        TokenKind::Equals => "=",
        TokenKind::OverrideEquals => ":=",
        TokenKind::Comma => ",",
        TokenKind::Semicolon => ";",
        TokenKind::Quote(_) => "'",
        TokenKind::String(_) => "quoted string",
        TokenKind::Word(_) => "value string",
    }
}

fn classify_word(raw: String, span: Span) -> Value {    if raw.eq_ignore_ascii_case("true") {
        return Value::boolean(true, span);
    }
    if raw.eq_ignore_ascii_case("false") {
        return Value::boolean(false, span);
    }
    if let Some((value, integer)) = parse_number(&raw) {
        return Value::number(raw, value, integer, span);
    }
    Value::new(
        ValueKind::String {
            value: raw.clone(),
            raw,
            quote: None,
        },
        span,
    )
}

/// Parse a HIT number, including scientific notation. Returns `(value,
/// syntactically_integer)`.
fn parse_number(raw: &str) -> Option<(f64, bool)> {
    if raw.is_empty() || !raw.is_ascii() {
        return None;
    }

    let bytes = raw.as_bytes();
    let mut index = 0;

    if matches!(bytes.get(index), Some(b'+') | Some(b'-')) {
        index += 1;
    }

    let mut mantissa_digits = 0;
    while matches!(bytes.get(index), Some(b'0'..=b'9')) {
        index += 1;
        mantissa_digits += 1;
    }

    if matches!(bytes.get(index), Some(b'.')) {
        index += 1;
        while matches!(bytes.get(index), Some(b'0'..=b'9')) {
            index += 1;
            mantissa_digits += 1;
        }
    }

    if mantissa_digits == 0 {
        return None;
    }

    if matches!(bytes.get(index), Some(b'e') | Some(b'E')) {
        index += 1;
        if matches!(bytes.get(index), Some(b'+') | Some(b'-')) {
            index += 1;
        }
        let mut exponent_digits = 0;
        while matches!(bytes.get(index), Some(b'0'..=b'9')) {
            index += 1;
            exponent_digits += 1;
        }
        if exponent_digits == 0 {
            return None;
        }
    }

    if index != bytes.len() {
        return None;
    }

    let value = raw.parse::<f64>().ok()?;
    if !value.is_finite() {
        // JSON cannot represent infinities or NaN. Keep the spelling as a
        // string value instead of failing later during serialization.
        return None;
    }
    let integer = !raw.contains('.') && !raw.contains('e') && !raw.contains('E');
    Some((value, integer))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_nested_blocks() {
        let document = parse(
            r#"
[GlobalParams]
  displacements = 'disp_x disp_y'
[]

[Mesh]
  type = GeneratedMesh
  dim = 2
  [./sub]
    nx = 10
  [../]
[]
"#,
        );
        assert!(document.diagnostics.is_empty(), "{:?}", document.diagnostics);
        assert_eq!(document.blocks().count(), 2);
        let mesh = document.block("Mesh").unwrap();
        assert_eq!(mesh.block_type.as_deref(), Some("GeneratedMesh"));
        assert_eq!(mesh.parameter("dim").unwrap().value.as_f64(), Some(2.0));
        let sub = mesh.block("sub").unwrap();
        assert_eq!(sub.path.as_deref(), Some("./sub"));
        assert_eq!(sub.parameter("nx").unwrap().value.as_f64(), Some(10.0));
    }

    #[test]
    fn parses_values_and_vectors() {
        let document = parse(
            r#"
[Block]
  flag = true
  other = FALSE
  x = -1.5e-3
  name = 'hello world'
  escaped = "a\"b"
  vec = 'a b c'
  arr = [1, 2, 3]
  spaced = 4 5 6
  nested = [[1, 2], [3, 4]]
[]
"#,
        );
        assert!(document.diagnostics.is_empty(), "{:?}", document.diagnostics);
        let block = document.block("Block").unwrap();
        assert_eq!(block.parameter("flag").unwrap().value.as_bool(), Some(true));
        assert_eq!(block.parameter("other").unwrap().value.as_bool(), Some(false));
        assert_eq!(block.parameter("x").unwrap().value.as_f64(), Some(-0.0015));
        assert_eq!(
            block.parameter("name").unwrap().value.as_str(),
            Some("hello world")
        );
        assert_eq!(
            block.parameter("escaped").unwrap().value.as_str(),
            Some("a\"b")
        );
        assert_eq!(block.parameter("vec").unwrap().value.as_str(), Some("a b c"));
        assert_eq!(
            block
                .parameter("arr")
                .unwrap()
                .value
                .array_values()
                .unwrap()
                .count(),
            3
        );
        assert_eq!(
            block
                .parameter("spaced")
                .unwrap()
                .value
                .array_values()
                .unwrap()
                .count(),
            3
        );
        assert_eq!(
            block
                .parameter("nested")
                .unwrap()
                .value
                .array_values()
                .unwrap()
                .count(),
            2
        );
    }

    #[test]
    fn parses_multiline_arrays_and_comments() {
        let document = parse(
            r#"
[Block]
  values = [
    # first group
    1, 2, 3, # trailing
    4,
  ]
[]
"#,
        );
        assert!(document.diagnostics.is_empty(), "{:?}", document.diagnostics);
        let value = &document.block("Block").unwrap().parameter("values").unwrap().value;
        assert!(value.is_multiline());
        let elements = value.as_array().unwrap();
        assert_eq!(
            elements
                .iter()
                .filter(|element| matches!(element, ArrayElement::Value(_)))
                .count(),
            4
        );
        assert_eq!(
            elements
                .iter()
                .filter(|element| matches!(element, ArrayElement::Comment(_)))
                .count(),
            2
        );
    }

    #[test]
    fn parses_empty_arrays_as_elements_on_their_own_line() {
        let document = parse(
            r#"
[Block]
  matrix = [
    []
  ]
  rows = [
    []
    [1, 2]
    []
    # between rows
    []
  ]
[]
"#,
        );
        assert!(document.diagnostics.is_empty(), "{:?}", document.diagnostics);
        let block = document.block("Block").unwrap();

        let matrix = block.parameter("matrix").unwrap().value.as_array().unwrap();
        assert_eq!(matrix.len(), 1);
        let ArrayElement::Value(empty) = &matrix[0] else {
            panic!("expected an array value, got {matrix:?}");
        };
        assert_eq!(empty.as_array().unwrap().len(), 0);

        let rows = block.parameter("rows").unwrap().value.as_array().unwrap();
        let values: Vec<_> = rows
            .iter()
            .filter_map(|element| match element {
                ArrayElement::Value(value) => Some(value),
                _ => None,
            })
            .collect();
        assert_eq!(values.len(), 4);
        assert_eq!(values[0].as_array().unwrap().len(), 0);
        assert_eq!(values[1].as_array().unwrap().len(), 2);
        assert_eq!(values[2].as_array().unwrap().len(), 0);
        assert_eq!(values[3].as_array().unwrap().len(), 0);
    }

    #[test]
    fn reports_unclosed_array_before_block_close() {
        let document = parse(
            r#"
[Block]
  values = [
    1
    2
[]
"#,
        );
        assert!(
            document
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "unclosed-array"),
            "{:?}",
            document.diagnostics
        );
    }

    #[test]
    fn preserves_inline_and_block_comments() {
        let document = parse(
            r#"# file header
[Block] # header comment
  a = 1 # inline
  # own line
  b = 2
[]
"#,
        );
        assert!(document.diagnostics.is_empty(), "{:?}", document.diagnostics);
        assert_eq!(document.comments().count(), 1);
        let block = document.block("Block").unwrap();
        let comments: Vec<_> = block.comments().collect();
        assert_eq!(comments.len(), 3);
        assert!(comments[0].inline);
        assert_eq!(comments[0].text, " header comment");
        assert!(comments[1].inline);
        assert_eq!(comments[1].text, " inline");
        assert!(!comments[2].inline);
        assert_eq!(comments[2].text, " own line");
    }

    #[test]
    fn reports_missing_equals_and_recovers() {
        let document = parse(
            r#"
[Block]
  broken
  good = 1
[]
"#,
        );
        assert_eq!(document.diagnostics.len(), 1);
        assert_eq!(document.diagnostics[0].code, "missing-equals");
        assert_eq!(document.diagnostics[0].line, 3);
        let block = document.block("Block").unwrap();
        assert_eq!(block.parameter("good").unwrap().value.as_f64(), Some(1.0));
    }

    #[test]
    fn reports_unclosed_block_and_header() {
        let document = parse(
            r#"
[Block
  a = 1
"#,
        );
        let codes: Vec<_> = document
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code.as_str())
            .collect();
        assert!(codes.contains(&"unclosed-block-header"));
        assert!(codes.contains(&"unclosed-block"));
        assert_eq!(
            document
                .block("Block")
                .unwrap()
                .parameter("a")
                .unwrap()
                .value
                .as_f64(),
            Some(1.0)
        );
    }

    #[test]
    fn recovers_from_unclosed_array_before_block_close() {
        let document = parse(
            r#"
[Block]
  values = [1, 2
[]
"#,
        );
        assert!(document
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "unclosed-array"));
        assert_eq!(document.blocks().count(), 1);
    }

    #[test]
    fn reports_unexpected_closing_block() {
        let document = parse("[]\na = 1\n");
        assert_eq!(document.diagnostics[0].code, "unexpected-closing-block");
        assert_eq!(document.parameter("a").unwrap().value.as_f64(), Some(1.0));
    }

    #[test]
    fn reports_duplicate_parameters_as_warnings() {
        let document = parse("[Block]\n  a = 1\n  a = 2\n[]\n");
        assert_eq!(document.diagnostics.len(), 1);
        assert_eq!(document.diagnostics[0].severity, Severity::Warning);
        assert_eq!(document.diagnostics[0].code, "duplicate-parameter");
    }

    #[test]
    fn tracks_spans() {
        let document = parse("[Block]\n  a = 1\n[]\n");
        let block = document.block("Block").unwrap();
        assert_eq!(block.span.start.line, 1);
        assert_eq!(block.span.start.column, 1);
        assert_eq!(block.header_span.start.offset, 0);
        let parameter = block.parameter("a").unwrap();
        assert_eq!(parameter.key_span.start.line, 2);
        assert_eq!(parameter.key_span.start.column, 3);
        assert_eq!(parameter.value_span.start.column, 7);
    }

    #[test]
    fn number_grammar() {
        assert_eq!(parse_number("1"), Some((1.0, true)));
        assert_eq!(parse_number("-1.5e-3"), Some((-0.0015, false)));
        assert_eq!(parse_number("+2E4"), Some((20000.0, false)));
        assert_eq!(parse_number(".5"), Some((0.5, false)));
        assert_eq!(parse_number("1."), Some((1.0, false)));
        assert_eq!(parse_number("1e"), None);
        assert_eq!(parse_number("e10"), None);
        assert_eq!(parse_number("1st"), None);
        assert_eq!(parse_number("0x10"), None);
        assert_eq!(parse_number("1e999"), None);
    }
}
