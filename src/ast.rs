//! Serializable abstract syntax tree for the MOOSE HIT input format.
//!
//! The AST deliberately keeps the source ordering of items. A HIT file is a
//! sequence of parameters, blocks and comments, and preserving that sequence is
//! what makes a near-lossless round-trip possible.
//!
//! All line/column numbers are **1-based** and count Unicode scalar values
//! (Rust `char`s), while `offset` is a **0-based byte offset** into the source.
//! The `end` position of a span is exclusive.

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// A position in the source text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
pub struct Position {
    /// 1-based line number.
    #[cfg_attr(feature = "serde", serde(default))]
    pub line: u32,
    /// 1-based column number, measured in Unicode scalar values.
    #[cfg_attr(feature = "serde", serde(default))]
    pub column: u32,
    /// 0-based byte offset into the source.
    #[cfg_attr(feature = "serde", serde(default))]
    pub offset: u32,
}

impl Position {
    pub const fn new(line: u32, column: u32, offset: u32) -> Self {
        Self {
            line,
            column,
            offset,
        }
    }
}

/// A half-open source range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
pub struct Span {
    #[cfg_attr(feature = "serde", serde(default))]
    pub start: Position,
    #[cfg_attr(feature = "serde", serde(default))]
    pub end: Position,
}

impl Span {
    pub const fn new(start: Position, end: Position) -> Self {
        Self { start, end }
    }

    pub const fn point(position: Position) -> Self {
        Self {
            start: position,
            end: position,
        }
    }

    pub fn merge(self, other: Self) -> Self {
        let start = if self.start.offset <= other.start.offset {
            self.start
        } else {
            other.start
        };
        let end = if self.end.offset >= other.end.offset {
            self.end
        } else {
            other.end
        };
        Self { start, end }
    }

    pub fn is_empty(self) -> bool {
        self.start.offset == self.end.offset
    }

    pub fn line(self) -> u32 {
        self.start.line
    }

    pub fn column(self) -> u32 {
        self.start.column
    }
}

/// A complete HIT document.
#[derive(Debug, Clone, PartialEq, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
pub struct Document {
    /// Parameters, blocks and comments in source order.
    #[cfg_attr(feature = "serde", serde(default))]
    pub items: Vec<Item>,
    /// Span covering the whole document (empty documents are a point span).
    #[cfg_attr(feature = "serde", serde(default))]
    pub span: Span,
    /// Diagnostics produced while parsing. Recoverable syntax errors do not
    /// prevent an AST from being returned.
    #[cfg_attr(feature = "serde", serde(default))]
    pub diagnostics: Vec<Diagnostic>,
}

impl Document {
    pub fn new(items: Vec<Item>, span: Span, diagnostics: Vec<Diagnostic>) -> Self {
        Self {
            items,
            span,
            diagnostics,
        }
    }

    /// Iterate over top-level parameters.
    pub fn parameters(&self) -> impl Iterator<Item = &Parameter> {
        self.items.iter().filter_map(|item| match item {
            Item::Parameter(p) => Some(p),
            _ => None,
        })
    }

    /// Iterate over top-level blocks.
    pub fn blocks(&self) -> impl Iterator<Item = &Block> {
        self.items.iter().filter_map(|item| match item {
            Item::Block(b) => Some(b),
            _ => None,
        })
    }

    /// Iterate over top-level comments.
    pub fn comments(&self) -> impl Iterator<Item = &Comment> {
        self.items.iter().filter_map(|item| match item {
            Item::Comment(c) => Some(c),
            _ => None,
        })
    }

    /// Find the first top-level block with the given display name.
    pub fn block(&self, name: &str) -> Option<&Block> {
        self.blocks().find(|block| block.name == name)
    }

    /// Find the first top-level parameter with the given key.
    pub fn parameter(&self, key: &str) -> Option<&Parameter> {
        self.parameters().find(|param| param.key == key)
    }

    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == Severity::Error)
    }
}

/// An ordered document item.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(tag = "kind", rename_all = "camelCase"))]
pub enum Item {
    Parameter(Parameter),
    Block(Block),
    Comment(Comment),
}

impl Item {
    pub fn span(&self) -> Span {
        match self {
            Item::Parameter(parameter) => parameter.span,
            Item::Block(block) => block.span,
            Item::Comment(comment) => comment.span,
        }
    }

    pub fn as_parameter(&self) -> Option<&Parameter> {
        match self {
            Item::Parameter(parameter) => Some(parameter),
            _ => None,
        }
    }

    pub fn as_block(&self) -> Option<&Block> {
        match self {
            Item::Block(block) => Some(block),
            _ => None,
        }
    }

    pub fn as_comment(&self) -> Option<&Comment> {
        match self {
            Item::Comment(comment) => Some(comment),
            _ => None,
        }
    }
}

/// A `key = value` assignment.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
pub struct Parameter {
    /// Parameter name as written (without surrounding quotes).
    pub key: String,
    pub value: Value,
    /// Span covering `key` through the end of the value.
    #[cfg_attr(feature = "serde", serde(default))]
    pub span: Span,
    #[cfg_attr(feature = "serde", serde(default))]
    pub key_span: Span,
    #[cfg_attr(feature = "serde", serde(default))]
    pub value_span: Span,
    /// True when the assignment used WASP's override spelling (`:=` or
    /// `:override=`).
    #[cfg_attr(feature = "serde", serde(default))]
    pub override_assign: bool,
    /// The assignment spelling as written: `=`, `:=` or `:override=`.
    #[cfg_attr(feature = "serde", serde(default = "default_assign"))]
    pub assign: String,
}

fn default_assign() -> String {
    "=".to_string()
}

impl Parameter {
    pub fn new(key: impl Into<String>, value: Value, span: Span, key_span: Span, value_span: Span) -> Self {
        Self {
            key: key.into(),
            value,
            span,
            key_span,
            value_span,
            override_assign: false,
            assign: default_assign(),
        }
    }

    /// The parameter value as a string, if it is a string value.
    pub fn as_str(&self) -> Option<&str> {
        self.value.as_str()
    }
}

/// A `[name]` section. Blocks may contain parameters, nested blocks and
/// comments, in source order.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
pub struct Block {
    /// Display name, i.e. the last path component. `[./my_mesh]` becomes
    /// `my_mesh`; `[Mesh]` becomes `Mesh`.
    pub name: String,
    /// Relative or multi-component path as written, e.g. `./my_mesh` or
    /// `Mesh/Sub`. `None` for a simple name.
    #[cfg_attr(feature = "serde", serde(default))]
    pub path: Option<String>,
    /// The exact text between the brackets, trimmed. This is what the
    /// serializer writes back, so dot-prefixed names round-trip.
    #[cfg_attr(feature = "serde", serde(default))]
    pub raw_header: String,
    /// Convenience mirror of a direct `type = ...` parameter, when present.
    #[cfg_attr(feature = "serde", serde(default))]
    pub block_type: Option<String>,
    #[cfg_attr(feature = "serde", serde(default))]
    pub items: Vec<Item>,
    #[cfg_attr(feature = "serde", serde(default))]
    pub span: Span,
    #[cfg_attr(feature = "serde", serde(default))]
    pub header_span: Span,
    /// Raw closing header, e.g. `[]` or `[../]`. `None` for an unclosed block.
    #[cfg_attr(feature = "serde", serde(default))]
    pub closing: Option<String>,
    #[cfg_attr(feature = "serde", serde(default))]
    pub close_span: Option<Span>,
}

impl Block {
    pub fn new(
        name: impl Into<String>,
        raw_header: impl Into<String>,
        items: Vec<Item>,
        span: Span,
        header_span: Span,
        closing: Option<String>,
        close_span: Option<Span>,
    ) -> Self {
        let raw_header = raw_header.into();
        let (_, path) = split_header(&raw_header);
        let block_type = find_block_type(&items);
        Self {
            name: name.into(),
            path,
            raw_header,
            block_type,
            items,
            span,
            header_span,
            closing,
            close_span,
        }
    }

    pub fn parameters(&self) -> impl Iterator<Item = &Parameter> {
        self.items.iter().filter_map(|item| match item {
            Item::Parameter(parameter) => Some(parameter),
            _ => None,
        })
    }

    pub fn blocks(&self) -> impl Iterator<Item = &Block> {
        self.items.iter().filter_map(|item| match item {
            Item::Block(block) => Some(block),
            _ => None,
        })
    }

    pub fn comments(&self) -> impl Iterator<Item = &Comment> {
        self.items.iter().filter_map(|item| match item {
            Item::Comment(comment) => Some(comment),
            _ => None,
        })
    }

    pub fn parameter(&self, key: &str) -> Option<&Parameter> {
        self.parameters().find(|parameter| parameter.key == key)
    }

    pub fn block(&self, name: &str) -> Option<&Block> {
        self.blocks().find(|block| block.name == name)
    }

    /// The header that should be written between `[` and `]`.
    pub fn header(&self) -> &str {
        if !self.raw_header.is_empty() {
            &self.raw_header
        } else if let Some(path) = &self.path {
            path
        } else {
            &self.name
        }
    }
}

/// A human comment. `text` excludes the leading `#` but retains all spacing
/// after it, so `# hello` round-trips as `text == " hello"`.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
pub struct Comment {
    pub text: String,
    /// True when the comment shared a line with the preceding item.
    #[cfg_attr(feature = "serde", serde(default))]
    pub inline: bool,
    #[cfg_attr(feature = "serde", serde(default))]
    pub span: Span,
}

impl Comment {
    pub fn new(text: impl Into<String>, inline: bool, span: Span) -> Self {
        Self {
            text: text.into(),
            inline,
            span,
        }
    }

    /// The comment including its leading `#`.
    pub fn raw(&self) -> String {
        format!("#{}", self.text)
    }
}

/// A value plus its source span.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
pub struct Value {
    #[cfg_attr(feature = "serde", serde(flatten))]
    pub kind: ValueKind,
    #[cfg_attr(feature = "serde", serde(default))]
    pub span: Span,
}

impl Value {
    pub fn new(kind: ValueKind, span: Span) -> Self {
        Self { kind, span }
    }

    pub fn string(value: impl Into<String>, span: Span) -> Self {
        let value = value.into();
        Self::new(
            ValueKind::String {
                raw: value.clone(),
                value,
                quote: None,
            },
            span,
        )
    }

    pub fn quoted_string(
        value: impl Into<String>,
        raw: impl Into<String>,
        quote: char,
        span: Span,
    ) -> Self {
        Self::new(
            ValueKind::String {
                value: value.into(),
                raw: raw.into(),
                quote: Some(quote),
            },
            span,
        )
    }

    pub fn number(raw: impl Into<String>, value: f64, integer: bool, span: Span) -> Self {
        Self::new(
            ValueKind::Number {
                raw: raw.into(),
                value,
                integer,
            },
            span,
        )
    }

    pub fn boolean(value: bool, span: Span) -> Self {
        Self::new(ValueKind::Boolean { value }, span)
    }

    pub fn array(
        elements: Vec<ArrayElement>,
        multiline: bool,
        trailing_comma: bool,
        span: Span,
    ) -> Self {
        Self::new(
            ValueKind::Array {
                elements,
                multiline,
                trailing_comma,
                quoted: false,
            },
            span,
        )
    }

    /// A single-quoted WASP-style array (`'a b;c'`). Quote marks and
    /// semicolons are retained as [`ArrayElement`]s so the original structure
    /// round-trips and the conformance projection can report every part.
    pub fn quoted_array(elements: Vec<ArrayElement>, span: Span) -> Self {
        Self::new(
            ValueKind::Array {
                elements,
                multiline: false,
                trailing_comma: false,
                quoted: true,
            },
            span,
        )
    }

    pub fn missing(span: Span) -> Self {
        Self::new(ValueKind::Missing, span)
    }

    pub fn as_str(&self) -> Option<&str> {
        match &self.kind {
            ValueKind::String { value, .. } => Some(value),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match &self.kind {
            ValueKind::Number { value, .. } => Some(*value),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match &self.kind {
            ValueKind::Boolean { value } => Some(*value),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&[ArrayElement]> {
        match &self.kind {
            ValueKind::Array { elements, .. } => Some(elements),
            _ => None,
        }
    }

    /// Values in an array, skipping comments and structural markers.
    pub fn array_values(&self) -> Option<impl Iterator<Item = &Value>> {
        self.as_array().map(|elements| {
            elements.iter().filter_map(|element| match element {
                ArrayElement::Value(value) => Some(value),
                _ => None,
            })
        })
    }

    pub fn is_missing(&self) -> bool {
        matches!(self.kind, ValueKind::Missing)
    }

    pub fn is_multiline(&self) -> bool {
        matches!(self.kind, ValueKind::Array { multiline: true, .. })
    }
}

/// The kind of a value.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(tag = "kind", rename_all = "camelCase"))]
pub enum ValueKind {
    /// Quoted or bare string. `quote` is `None` for unquoted text.
    String {
        value: String,
        #[cfg_attr(feature = "serde", serde(default))]
        raw: String,
        #[cfg_attr(feature = "serde", serde(default))]
        quote: Option<char>,
    },
    /// A number. `raw` preserves the exact spelling (scientific notation,
    /// leading `+`, etc.) for lossless formatting.
    Number {
        #[cfg_attr(feature = "serde", serde(default))]
        raw: String,
        #[cfg_attr(feature = "serde", serde(default))]
        value: f64,
        #[cfg_attr(feature = "serde", serde(default))]
        integer: bool,
    },
    Boolean {
        value: bool,
    },
    /// A bracket array (`[1, 2, 3]`), an unquoted vector
    /// (`values = 1 2 3`), or a WASP single-quoted array (`'1 2 3'`).
    Array {
        #[cfg_attr(feature = "serde", serde(default))]
        elements: Vec<ArrayElement>,
        #[cfg_attr(feature = "serde", serde(default))]
        multiline: bool,
        #[cfg_attr(feature = "serde", serde(default))]
        trailing_comma: bool,
        /// True for WASP-style single-quoted arrays. Such arrays retain
        /// [`ArrayElement::Quote`] and [`ArrayElement::Semicolon`] markers.
        #[cfg_attr(feature = "serde", serde(default))]
        quoted: bool,
    },
    /// Placeholder inserted after a syntax error so parsing can continue.
    Missing,
}

/// An array element: a value, a comment, or (for WASP-style quoted arrays) a
/// structural quote or semicolon marker.
///
/// Array elements use external tagging (`{"value": {...}}` /
/// `{"comment": {...}}`) so the `kind` tag of a nested [`Value`] can stay
/// flattened.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
pub enum ArrayElement {
    Value(Value),
    Comment(Comment),
    /// An opening or closing single quote in a WASP-style array.
    Quote(QuoteMark),
    /// A `;` separator in a WASP-style array.
    Semicolon(Span),
}

/// A single quote mark retained as an array part.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
pub struct QuoteMark {
    pub mark: char,
    #[cfg_attr(feature = "serde", serde(default))]
    pub span: Span,
}

impl QuoteMark {
    pub fn new(mark: char, span: Span) -> Self {
        Self { mark, span }
    }
}

impl ArrayElement {
    pub fn span(&self) -> Span {
        match self {
            ArrayElement::Value(value) => value.span,
            ArrayElement::Comment(comment) => comment.span,
            ArrayElement::Quote(quote) => quote.span,
            ArrayElement::Semicolon(span) => *span,
        }
    }

    pub fn as_value(&self) -> Option<&Value> {
        match self {
            ArrayElement::Value(value) => Some(value),
            _ => None,
        }
    }
}

/// Severity of a [`Diagnostic`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "lowercase"))]
pub enum Severity {
    Error,
    Warning,
    Info,
}

impl Severity {
    pub fn as_str(self) -> &'static str {
        match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Info => "info",
        }
    }
}

/// A syntax error or lint warning with a source location.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
pub struct Diagnostic {
    pub severity: Severity,
    /// Stable machine-readable code, e.g. `missing-equals`.
    pub code: String,
    pub message: String,
    #[cfg_attr(feature = "serde", serde(default))]
    pub span: Span,
    /// Convenience copy of `span.start.line` (1-based).
    #[cfg_attr(feature = "serde", serde(default))]
    pub line: u32,
    /// Convenience copy of `span.start.column` (1-based).
    #[cfg_attr(feature = "serde", serde(default))]
    pub column: u32,
}

impl Diagnostic {
    pub fn new(
        severity: Severity,
        code: impl Into<String>,
        message: impl Into<String>,
        span: Span,
    ) -> Self {
        Self {
            severity,
            code: code.into(),
            message: message.into(),
            span,
            line: span.start.line,
            column: span.start.column,
        }
    }

    pub fn error(code: impl Into<String>, message: impl Into<String>, span: Span) -> Self {
        Self::new(Severity::Error, code, message, span)
    }

    pub fn warning(code: impl Into<String>, message: impl Into<String>, span: Span) -> Self {
        Self::new(Severity::Warning, code, message, span)
    }

    pub fn info(code: impl Into<String>, message: impl Into<String>, span: Span) -> Self {
        Self::new(Severity::Info, code, message, span)
    }
}

/// Split a raw header into a display name and an optional path.
///
/// ```text
/// Mesh          -> ("Mesh", None)
/// ./my_mesh     -> ("my_mesh", Some("./my_mesh"))
/// Mesh/Sub      -> ("Sub", Some("Mesh/Sub"))
/// ```
pub fn split_header(raw: &str) -> (String, Option<String>) {
    let trimmed = raw.trim();
    let name = trimmed
        .rsplit('/')
        .next()
        .unwrap_or(trimmed)
        .trim_start_matches('.')
        .to_string();
    let is_path = trimmed.contains('/') || trimmed.starts_with('.');
    let path = if is_path && !trimmed.is_empty() {
        Some(trimmed.to_string())
    } else {
        None
    };
    (name, path)
}

fn find_block_type(items: &[Item]) -> Option<String> {
    items.iter().find_map(|item| match item {
        Item::Parameter(parameter) if parameter.key == "type" => match &parameter.value.kind {
            ValueKind::String { value, .. } => Some(value.clone()),
            ValueKind::Number { raw, .. } => Some(raw.clone()),
            _ => None,
        },
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_headers() {
        assert_eq!(split_header("Mesh"), ("Mesh".into(), None));
        assert_eq!(
            split_header("./my_mesh"),
            ("my_mesh".into(), Some("./my_mesh".into()))
        );
        assert_eq!(
            split_header("Mesh/Sub"),
            ("Sub".into(), Some("Mesh/Sub".into()))
        );
        assert_eq!(
            split_header("../sibling"),
            ("sibling".into(), Some("../sibling".into()))
        );
    }

    #[test]
    fn span_merge_and_helpers() {
        let a = Span::new(Position::new(1, 1, 0), Position::new(1, 4, 3));
        let b = Span::new(Position::new(2, 1, 5), Position::new(2, 2, 6));
        let merged = a.merge(b);
        assert_eq!(merged.start, a.start);
        assert_eq!(merged.end, b.end);
        assert_eq!(merged.line(), 1);
        assert_eq!(merged.column(), 1);
        assert!(!merged.is_empty());
    }
}
