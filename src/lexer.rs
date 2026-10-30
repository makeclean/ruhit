//! HIT tokenizer.
//!
//! The lexer is intentionally permissive: unknown characters become part of a
//! word, and malformed strings produce a diagnostic instead of a panic. The
//! parser is responsible for deciding what the tokens mean.

use crate::ast::{Diagnostic, Position, Span};

/// A token produced by the lexer.
#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

/// A decoded quoted string, retaining its original spelling for formatting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StringLiteral {
    pub value: String,
    pub raw: String,
    pub quote: char,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    LBracket,
    RBracket,
    Equals,
    /// WASP's override assignment spelling (`:=` or `:override=`).
    OverrideEquals,
    Comma,
    /// A single quote in WASP-compat mode. Opens or closes a quoted array.
    Quote(char),
    /// A `;` inside a WASP-compat quoted array.
    Semicolon,
    Newline,
    /// Comment text after `#` (the `#` itself is not included).
    Comment(String),
    String(StringLiteral),
    /// Any unquoted run of non-delimiter characters. In WASP-compat mode this
    /// may contain embedded `${...}` expressions, including spaces.
    Word(String),
    Eof,
}

impl TokenKind {
    pub fn describe(&self) -> &'static str {
        match self {
            TokenKind::LBracket => "'['",
            TokenKind::RBracket => "']'",
            TokenKind::Equals => "'='",
            TokenKind::OverrideEquals => "':='",
            TokenKind::Comma => "','",
            TokenKind::Quote(_) => "'''",
            TokenKind::Semicolon => "';'",
            TokenKind::Newline => "end of line",
            TokenKind::Comment(_) => "comment",
            TokenKind::String(_) => "string",
            TokenKind::Word(_) => "word",
            TokenKind::Eof => "end of file",
        }
    }
}

/// Lexer behaviour switches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LexOptions {
    /// Enable WASP HIT compatibility: single quotes delimit arrays, `;`
    /// separates array values, `:=`/`:override=` are assignments and
    /// `${...}` expressions are captured as opaque values.
    pub wasp_compat: bool,
}

impl LexOptions {
    pub const fn wasp() -> Self {
        Self { wasp_compat: true }
    }
}

/// The result of tokenizing a source string.
#[derive(Debug, Clone, PartialEq)]
pub struct Lexed {
    pub tokens: Vec<Token>,
    pub diagnostics: Vec<Diagnostic>,
}

/// Tokenize HIT source text with the default (lenient) options.
pub fn tokenize(source: &str) -> Lexed {
    tokenize_with(source, LexOptions::default())
}

/// Tokenize HIT source text with explicit options.
pub fn tokenize_with(source: &str, options: LexOptions) -> Lexed {
    Lexer::new(source, options).run()
}

struct Lexer<'a> {
    source: &'a str,
    pos: usize,
    line: u32,
    column: u32,
    options: LexOptions,
    in_array: bool,
    tokens: Vec<Token>,
    diagnostics: Vec<Diagnostic>,
}

impl<'a> Lexer<'a> {
    fn new(source: &'a str, options: LexOptions) -> Self {
        // Skip a UTF-8 byte-order mark without shifting the first token's
        // column. Offsets still refer to the original source bytes.
        let pos = if source.starts_with('\u{feff}') {
            '\u{feff}'.len_utf8()
        } else {
            0
        };
        Self {
            source,
            pos,
            line: 1,
            column: 1,
            options,
            in_array: false,
            tokens: Vec::new(),
            diagnostics: Vec::new(),
        }
    }

    fn run(mut self) -> Lexed {
        while let Some(ch) = self.peek() {
            let start = self.position();
            if self.options.wasp_compat && self.in_array {
                self.run_array_state(ch, start);
                continue;
            }
            match ch {
                ' ' | '\t' | '\u{feff}' => {
                    self.bump();
                }
                '\n' => {
                    self.bump();
                    self.push(TokenKind::Newline, start);
                }
                '\r' => {
                    self.bump();
                    if self.peek() == Some('\n') {
                        self.bump();
                    } else {
                        self.line += 1;
                        self.column = 1;
                    }
                    self.push(TokenKind::Newline, start);
                }
                '#' => {
                    self.bump();
                    let text_start = self.pos;
                    while let Some(ch) = self.peek() {
                        if ch == '\n' || ch == '\r' {
                            break;
                        }
                        self.bump();
                    }
                    let text = self.source[text_start..self.pos].to_string();
                    self.push(TokenKind::Comment(text), start);
                }
                '[' => {
                    self.bump();
                    self.push(TokenKind::LBracket, start);
                }
                ']' => {
                    self.bump();
                    self.push(TokenKind::RBracket, start);
                }
                '=' => {
                    self.bump();
                    self.push(TokenKind::Equals, start);
                }
                ':' if self.options.wasp_compat && self.starts_override_assign() => {
                    self.lex_override_assign(start);
                }
                ';' if self.options.wasp_compat => {
                    self.bump();
                    self.push(TokenKind::Semicolon, start);
                }
                ',' => {
                    self.bump();
                    self.push(TokenKind::Comma, start);
                }
                '\'' if self.options.wasp_compat => {
                    self.bump();
                    self.in_array = true;
                    self.push(TokenKind::Quote('\''), start);
                }
                '\'' | '"' => {
                    self.lex_string(ch, start);
                }
                _ => {
                    self.lex_word(start);
                }
            }
        }

        let position = self.position();
        self.tokens.push(Token {
            kind: TokenKind::Eof,
            span: Span::point(position),
        });

        Lexed {
            tokens: self.tokens,
            diagnostics: self.diagnostics,
        }
    }

    fn position(&self) -> Position {
        Position::new(self.line, self.column, self.pos as u32)
    }

    fn peek(&self) -> Option<char> {
        self.source[self.pos..].chars().next()
    }

    fn bump(&mut self) -> Option<char> {
        let ch = self.peek()?;
        self.pos += ch.len_utf8();
        if ch == '\n' {
            self.line += 1;
            self.column = 1;
        } else {
            self.column += 1;
        }
        Some(ch)
    }

    fn push(&mut self, kind: TokenKind, start: Position) {
        let span = Span::new(start, self.position());
        self.tokens.push(Token { kind, span });
    }

    fn lex_word(&mut self, start: Position) {
        let word_start = self.pos;
        while let Some(ch) = self.peek() {
            if self.options.wasp_compat && ch == '$' && self.peek_next() == Some('{') {
                self.scan_brace_expression();
                continue;
            }
            let delimiter = if self.options.wasp_compat {
                // WASP's VALUE_STRING may contain quotes after the first
                // character; only the first character is restricted.
                // A `:` that begins an override assignment terminates the
                // preceding parameter name.
                is_wasp_word_delimiter(ch) || (ch == ':' && self.starts_override_assign())
            } else {
                is_word_delimiter(ch)
            };
            if delimiter {
                break;
            }
            self.bump();
        }
        let word = self.source[word_start..self.pos].to_string();
        self.push(TokenKind::Word(word), start);
    }

    fn peek_next(&self) -> Option<char> {
        let mut chars = self.source[self.pos..].chars();
        chars.next()?;
        chars.next()
    }

    fn starts_override_assign(&self) -> bool {
        let rest = &self.source[self.pos..];
        rest.starts_with(":=") || rest.starts_with(":override=")
    }

    fn lex_override_assign(&mut self, start: Position) {
        while let Some(ch) = self.peek() {
            self.bump();
            if ch == '=' {
                break;
            }
        }
        self.push(TokenKind::OverrideEquals, start);
    }

    /// Consume `${ ... }`, including nested `${...}` and embedded whitespace.
    /// The caller has already established that the cursor is on `$`.
    fn scan_brace_expression(&mut self) {
        self.bump(); // '$'
        self.bump(); // '{'
        let mut depth = 1usize;
        while let Some(ch) = self.peek() {
            match ch {
                '{' => {
                    depth += 1;
                    self.bump();
                }
                '}' => {
                    depth -= 1;
                    self.bump();
                    if depth == 0 {
                        break;
                    }
                }
                _ => {
                    self.bump();
                }
            }
        }
    }

    fn run_array_state(&mut self, ch: char, start: Position) {
        match ch {
            ' ' | '\t' | '\r' => {
                self.bump();
            }
            '\n' => {
                // WASP's lexer consumes newlines inside arrays.
                self.bump();
            }
            '\'' => {
                self.bump();
                self.in_array = false;
                self.push(TokenKind::Quote('\''), start);
            }
            ';' => {
                self.bump();
                self.push(TokenKind::Semicolon, start);
            }
            '"' => {
                self.lex_string('"', start);
            }
            _ => {
                self.lex_array_word(start);
            }
        }
    }

    fn lex_array_word(&mut self, start: Position) {
        let word_start = self.pos;
        while let Some(ch) = self.peek() {
            if ch == '\\' && self.peek_next() == Some('\'') {
                self.bump();
                self.bump();
                continue;
            }
            if ch == '$' && self.peek_next() == Some('{') {
                self.scan_brace_expression();
                continue;
            }
            if is_array_word_delimiter(ch) {
                break;
            }
            self.bump();
        }
        let word = self.source[word_start..self.pos].to_string();
        self.push(TokenKind::Word(word), start);
    }

    fn lex_string(&mut self, quote: char, start: Position) {
        // Consume the opening quote.
        self.bump();

        let mut value = String::new();
        let mut closed = false;

        loop {
            let Some(ch) = self.peek() else {
                break;
            };

            if ch == quote {
                self.bump();
                closed = true;
                break;
            }

            if ch == '\\' {
                let escape_start = self.position();
                self.bump();
                match self.peek() {
                    Some('n') => {
                        self.bump();
                        value.push('\n');
                    }
                    Some('t') => {
                        self.bump();
                        value.push('\t');
                    }
                    Some('r') => {
                        self.bump();
                        value.push('\r');
                    }
                    Some('0') => {
                        self.bump();
                        value.push('\0');
                    }
                    Some('\\') => {
                        self.bump();
                        value.push('\\');
                    }
                    Some('\'') => {
                        self.bump();
                        value.push('\'');
                    }
                    Some('"') => {
                        self.bump();
                        value.push('"');
                    }
                    Some('#') => {
                        self.bump();
                        value.push('#');
                    }
                    // Backslash-newline is a line continuation.
                    Some('\n') => {
                        self.bump();
                    }
                    Some('\r') => {
                        self.bump();
                        if self.peek() == Some('\n') {
                            self.bump();
                        } else {
                            self.line += 1;
                            self.column = 1;
                        }
                    }
                    Some(other) => {
                        self.bump();
                        // Preserve unknown escapes verbatim rather than failing.
                        value.push('\\');
                        value.push(other);
                        if !self.options.wasp_compat {
                            self.diagnostics.push(Diagnostic::warning(
                                "unknown-escape",
                                format!("unknown escape sequence '\\{other}'"),
                                Span::new(escape_start, self.position()),
                            ));
                        }
                    }
                    None => {
                        value.push('\\');
                    }
                }
                continue;
            }

            if ch == '\n' {
                self.bump();
                value.push('\n');
                continue;
            }

            if ch == '\r' {
                self.bump();
                if self.peek() == Some('\n') {
                    self.bump();
                    value.push('\n');
                } else {
                    self.line += 1;
                    self.column = 1;
                    value.push('\r');
                }
                continue;
            }

            self.bump();
            value.push(ch);
        }

        let raw = self.source[start.offset as usize..self.pos].to_string();
        let span = Span::new(start, self.position());

        if !closed {
            self.diagnostics.push(Diagnostic::error(
                "unterminated-string",
                "unterminated string literal; expected a closing quote",
                span,
            ));
        }

        self.tokens.push(Token {
            kind: TokenKind::String(StringLiteral { value, raw, quote }),
            span,
        });
    }
}

fn is_word_delimiter(ch: char) -> bool {
    ch.is_whitespace()
        || ch == '\u{feff}'
        || matches!(ch, '[' | ']' | '=' | ',' | '#' | '\'' | '"')
}

/// Delimiters inside a WASP single-quoted array. `=`, `#` and `$` are ordinary
/// array-string characters there.
fn is_array_word_delimiter(ch: char) -> bool {
    ch.is_whitespace()
        || ch == '\u{feff}'
        || matches!(ch, '"' | ';' | '\'')
}

/// Delimiters for an unquoted WASP value string. Quotes are only special at
/// the start of a token; after the first character they are ordinary text.
fn is_wasp_word_delimiter(ch: char) -> bool {
    ch.is_whitespace()
        || ch == '\u{feff}'
        || matches!(ch, '[' | ']' | '#' | '=' | ',')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::Severity;

    fn kinds(source: &str) -> Vec<TokenKind> {
        tokenize(source)
            .tokens
            .into_iter()
            .map(|token| token.kind)
            .collect()
    }

    #[test]
    fn tokenizes_blocks_parameters_and_comments() {
        let tokens = kinds("[Mesh]\n  type = GeneratedMesh # mesh type\n[]\n");
        assert!(matches!(tokens[0], TokenKind::LBracket));
        assert!(matches!(&tokens[1], TokenKind::Word(w) if w == "Mesh"));
        assert!(matches!(tokens[2], TokenKind::RBracket));
        assert!(matches!(tokens[3], TokenKind::Newline));
        assert!(matches!(&tokens[4], TokenKind::Word(w) if w == "type"));
        assert!(matches!(tokens[5], TokenKind::Equals));
        assert!(matches!(&tokens[6], TokenKind::Word(w) if w == "GeneratedMesh"));
        assert!(matches!(&tokens[7], TokenKind::Comment(c) if c == " mesh type"));
        assert!(matches!(tokens[8], TokenKind::Newline));
        assert!(matches!(tokens[9], TokenKind::LBracket));
        assert!(matches!(tokens[10], TokenKind::RBracket));
    }

    #[test]
    fn tokenizes_quoted_strings_with_escapes() {
        let lexed = tokenize(r"key = 'it\'s a # not comment'");
        let string = lexed
            .tokens
            .iter()
            .find_map(|token| match &token.kind {
                TokenKind::String(s) => Some(s),
                _ => None,
            })
            .expect("string token");
        assert_eq!(string.value, "it's a # not comment");
        assert_eq!(string.quote, '\'');
        assert_eq!(string.raw, r"'it\'s a # not comment'");
        assert!(lexed.diagnostics.is_empty());
    }

    #[test]
    fn supports_multiline_strings() {
        let lexed = tokenize("key = 'left\nright'");
        let string = lexed
            .tokens
            .iter()
            .find_map(|token| match &token.kind {
                TokenKind::String(s) => Some(s),
                _ => None,
            })
            .unwrap();
        assert_eq!(string.value, "left\nright");
    }

    #[test]
    fn reports_unterminated_strings() {
        let lexed = tokenize("key = 'oops");
        assert_eq!(lexed.diagnostics.len(), 1);
        assert_eq!(lexed.diagnostics[0].severity, Severity::Error);
        assert_eq!(lexed.diagnostics[0].code, "unterminated-string");
    }

    #[test]
    fn tracks_unicode_columns() {
        let lexed = tokenize("é = '✓'\nnext = 1");
        let next = lexed
            .tokens
            .iter()
            .find(|token| matches!(&token.kind, TokenKind::Word(w) if w == "next"))
            .unwrap();
        assert_eq!(next.span.start.line, 2);
        assert_eq!(next.span.start.column, 1);
        let equals = lexed
            .tokens
            .iter()
            .find(|token| matches!(token.kind, TokenKind::Equals))
            .unwrap();
        assert_eq!(equals.span.start.column, 3);
    }

    #[test]
    fn handles_crlf_newlines() {
        let lexed = tokenize("a = 1\r\nb = 2\r\n");
        let b = lexed
            .tokens
            .iter()
            .find(|token| matches!(&token.kind, TokenKind::Word(w) if w == "b"))
            .unwrap();
        assert_eq!(b.span.start.line, 2);
    }

    #[test]
    fn skips_a_leading_byte_order_mark() {
        let lexed = tokenize("\u{feff}[Mesh]\n[]\n");
        let first = &lexed.tokens[0];
        assert!(matches!(first.kind, TokenKind::LBracket));
        assert_eq!(first.span.start.line, 1);
        assert_eq!(first.span.start.column, 1);
        assert_eq!(first.span.start.offset, 3);
        assert!(lexed.diagnostics.is_empty());
    }
}
