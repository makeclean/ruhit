//! HIT formatter.
//!
//! The serializer writes clean, consistently indented HIT while preserving
//! comments, dot-prefixed headers, the original spelling of numbers, and the
//! quote style/escapes of string literals.

use crate::ast::{
    ArrayElement, Block, Comment, Document, Item, Parameter, Value, ValueKind,
};

/// Formatting options for [`serialize_with_options`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormatOptions {
    /// Number of spaces per indentation level. Defaults to 2.
    pub indent_width: usize,
    /// Emit a trailing newline at the end of the document. Defaults to true.
    pub final_newline: bool,
}

impl Default for FormatOptions {
    fn default() -> Self {
        Self {
            indent_width: 2,
            final_newline: true,
        }
    }
}

/// Serialize an AST to HIT using the default formatting options.
pub fn serialize(document: &Document) -> String {
    serialize_with_options(document, &FormatOptions::default())
}

/// Serialize an AST to HIT with explicit formatting options.
pub fn serialize_with_options(document: &Document, options: &FormatOptions) -> String {
    let mut serializer = Serializer {
        options,
        lines: Vec::new(),
    };
    serializer.write_items(&document.items, 0);
    serializer.finish()
}

struct Serializer<'a> {
    options: &'a FormatOptions,
    lines: Vec<String>,
}

impl<'a> Serializer<'a> {
    fn finish(self) -> String {
        if self.lines.is_empty() {
            return String::new();
        }
        let mut output = self.lines.join("\n");
        if self.options.final_newline {
            output.push('\n');
        }
        output
    }

    fn push_line(&mut self, level: usize, text: &str) {
        let mut line = String::with_capacity(self.options.indent_width * level + text.len());
        for _ in 0..level * self.options.indent_width {
            line.push(' ');
        }
        line.push_str(text);
        self.lines.push(line);
    }

    fn append_inline_comment(&mut self, comment: &Comment) {
        if let Some(last) = self.lines.last_mut() {
            last.push(' ');
            last.push('#');
            last.push_str(&comment.text);
        } else {
            self.push_line(0, &comment.raw());
        }
    }

    fn write_items(&mut self, items: &[Item], level: usize) {
        for item in items {
            match item {
                Item::Comment(comment) if comment.inline => {
                    self.append_inline_comment(comment);
                }
                Item::Comment(comment) => {
                    self.push_line(level, &comment.raw());
                }
                Item::Parameter(parameter) => {
                    self.write_parameter(parameter, level);
                }
                Item::Block(block) => {
                    self.write_block(block, level);
                }
            }
        }
    }

    fn write_block(&mut self, block: &Block, level: usize) {
        self.push_line(level, &format!("[{}]", block.header()));
        self.write_items(&block.items, level + 1);
        let closing = block.closing.as_deref().unwrap_or("");
        self.push_line(level, &format!("[{closing}]"));
    }

    fn write_parameter(&mut self, parameter: &Parameter, level: usize) {
        let key = format_key(&parameter.key);
        if parameter.value.is_missing() {
            self.push_line(level, &format!("{key} ="));
            return;
        }

        if self.value_needs_multiline(&parameter.value) {
            self.push_line(level, &format!("{key} = ["));
            if let ValueKind::Array {
                elements,
                trailing_comma,
                ..
            } = &parameter.value.kind
            {
                self.write_array_elements(elements, level + 1, *trailing_comma);
            }
            self.push_line(level, "]");
        } else {
            let value = self.format_value(&parameter.value);
            self.push_line(level, &format!("{key} = {value}"));
        }
    }

    fn write_array_elements(
        &mut self,
        elements: &[ArrayElement],
        level: usize,
        trailing_comma: bool,
    ) {
        let last_value_index = elements
            .iter()
            .rposition(|element| matches!(element, ArrayElement::Value(_)));

        for (index, element) in elements.iter().enumerate() {
            match element {
                ArrayElement::Comment(comment) if comment.inline => {
                    self.append_inline_comment(comment);
                }
                ArrayElement::Comment(comment) => {
                    self.push_line(level, &comment.raw());
                }
                ArrayElement::Value(value) => {
                    let comma = Some(index) != last_value_index || trailing_comma;
                    if self.value_needs_multiline(value) {
                        self.write_nested_array(value, level, comma);
                    } else {
                        let text = self.format_value(value);
                        self.push_line(
                            level,
                            &if comma { format!("{text},") } else { text },
                        );
                    }
                }
            }
        }
    }

    fn write_nested_array(&mut self, value: &Value, level: usize, comma: bool) {
        let ValueKind::Array {
            elements,
            trailing_comma,
            ..
        } = &value.kind
        else {
            let text = self.format_value(value);
            self.push_line(level, &if comma { format!("{text},") } else { text });
            return;
        };

        self.push_line(level, "[");
        self.write_array_elements(elements, level + 1, *trailing_comma);
        self.push_line(level, "]");
        if comma {
            if let Some(last) = self.lines.last_mut() {
                last.push(',');
            }
        }
    }

    fn format_value(&self, value: &Value) -> String {
        match &value.kind {
            ValueKind::String {
                value,
                raw,
                quote,
            } => {
                if let Some(quote_char) = quote {
                    // Preserve the exact source spelling (including escapes)
                    // when the raw literal is complete. A malformed literal
                    // (for example an unterminated string) is rebuilt so the
                    // formatter always emits valid HIT.
                    if raw_literal_is_complete(raw, *quote_char) {
                        return raw.clone();
                    }
                    return quote_string(value, *quote_char);
                }
                if needs_quotes(value) {
                    quote_string(value, '\'')
                } else {
                    value.clone()
                }
            }
            ValueKind::Number { raw, value, .. } => {
                if raw.is_empty() {
                    value.to_string()
                } else {
                    raw.clone()
                }
            }
            ValueKind::Boolean { value } => {
                if *value {
                    "true".to_string()
                } else {
                    "false".to_string()
                }
            }
            ValueKind::Array {
                elements,
                trailing_comma,
                ..
            } => {
                let values: Vec<String> = elements
                    .iter()
                    .filter_map(|element| match element {
                        ArrayElement::Value(value) => Some(self.format_value(value)),
                        ArrayElement::Comment(_) => None,
                    })
                    .collect();
                let trailing = if *trailing_comma && !values.is_empty() {
                    ","
                } else {
                    ""
                };
                format!("[{}{}]", values.join(", "), trailing)
            }
            ValueKind::Missing => String::new(),
        }
    }

    fn value_needs_multiline(&self, value: &Value) -> bool {
        match &value.kind {
            ValueKind::Array {
                elements,
                multiline,
                ..
            } => {
                if *multiline {
                    return true;
                }
                elements.iter().any(|element| match element {
                    ArrayElement::Comment(_) => true,
                    ArrayElement::Value(value) => self.value_needs_multiline(value),
                })
            }
            _ => false,
        }
    }
}

fn format_key(key: &str) -> String {
    if needs_quotes(key) {
        quote_string(key, '\'')
    } else {
        key.to_string()
    }
}

fn needs_quotes(value: &str) -> bool {
    if value.is_empty() {
        return true;
    }
    if value.eq_ignore_ascii_case("true") || value.eq_ignore_ascii_case("false") {
        return true;
    }
    if value.parse::<f64>().is_ok() {
        return true;
    }
    value.chars().any(|ch| {
        ch.is_control()
            || ch.is_whitespace()
            || matches!(
                ch,
                '#' | '[' | ']' | '=' | ',' | '\'' | '"' | '\\'
            )
    })
}

/// Check that a raw literal starts and ends with an *unescaped* quote.
fn raw_literal_is_complete(raw: &str, quote: char) -> bool {
    if raw.len() < 2 || !raw.starts_with(quote) || !raw.ends_with(quote) {
        return false;
    }

    // The final quote is escaped when it is preceded by an odd number of
    // backslashes.
    let bytes = raw.as_bytes();
    let mut backslashes = 0;
    let mut index = bytes.len() - 1;
    while index > 0 && bytes[index - 1] == b'\\' {
        backslashes += 1;
        index -= 1;
    }
    backslashes % 2 == 0
}

fn quote_string(value: &str, quote: char) -> String {
    let mut output = String::with_capacity(value.len() + 2);
    output.push(quote);
    for ch in value.chars() {
        match ch {
            '\\' => output.push_str("\\\\"),
            '\n' => output.push_str("\\n"),
            '\t' => output.push_str("\\t"),
            '\r' => output.push_str("\\r"),
            '\0' => output.push_str("\\0"),
            ch if ch == quote => {
                output.push('\\');
                output.push(ch);
            }
            ch => output.push(ch),
        }
    }
    output.push(quote);
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse;

    #[test]
    fn formats_nested_blocks() {
        let document = parse(
            r#"[Mesh]
type=GeneratedMesh
dim=2
[./sub]
nx=10
[../]
[]
"#,
        );
        let formatted = serialize(&document);
        assert_eq!(
            formatted,
            "[Mesh]\n  type = GeneratedMesh\n  dim = 2\n  [./sub]\n    nx = 10\n  [../]\n[]\n"
        );
    }

    #[test]
    fn preserves_comments_and_inline_comments() {
        let document = parse(
            r#"# header
[Mesh] # mesh
  type = GeneratedMesh # kind
  # note
  dim = 2
[]
"#,
        );
        let formatted = serialize(&document);
        assert_eq!(
            formatted,
            "# header\n[Mesh] # mesh\n  type = GeneratedMesh # kind\n  # note\n  dim = 2\n[]\n"
        );
    }

    #[test]
    fn formats_multiline_arrays() {
        let document = parse(
            r#"[Block]
  values = [
    1, 2, # first
    3,
  ]
[]
"#,
        );
        let formatted = serialize(&document);
        assert_eq!(
            formatted,
            "[Block]\n  values = [\n    1,\n    2, # first\n    3,\n  ]\n[]\n"
        );
    }

    #[test]
    fn round_trip_is_stable() {
        let source = r#"# top
[GlobalParams]
  displacements = 'disp_x disp_y'
[]

[Mesh]
  type = GeneratedMesh
  dim = 2
  nx = 10
  ny = 10
  [./sub]
    values = [1, 2, 3]
  [../]
[]
"#;
        let first = serialize(&parse(source));
        let second = serialize(&parse(&first));
        assert_eq!(first, second);
    }

    #[test]
    fn quotes_strings_that_would_change_type() {
        let document = parse("[Block]\n  a = 'true'\n  b = '12'\n  c = plain\n[]\n");
        let formatted = serialize(&document);
        assert!(formatted.contains("a = 'true'"));
        assert!(formatted.contains("b = '12'"));
        assert!(formatted.contains("c = plain"));
    }

    #[test]
    fn custom_indentation_and_no_final_newline() {
        let document = parse("[Block]\n a = 1\n[]\n");
        let formatted = serialize_with_options(
            &document,
            &FormatOptions {
                indent_width: 4,
                final_newline: false,
            },
        );
        assert_eq!(formatted, "[Block]\n    a = 1\n[]");
    }

    #[test]
    fn quotes_parameter_keys_that_need_it() {
        let document = parse("'my key' = 1\n'odd=key' = 2\nplain = 3\n");
        let formatted = serialize(&document);
        assert!(formatted.contains("'my key' = 1"), "{formatted}");
        assert!(formatted.contains("'odd=key' = 2"), "{formatted}");
        assert!(formatted.contains("plain = 3"), "{formatted}");
        assert_eq!(serialize(&parse(&formatted)), formatted);
    }

    #[test]
    fn preserves_inline_trailing_comma() {
        let document = parse("[Block]\n  values = [1, 2,]\n[]\n");
        assert!(document.diagnostics.is_empty());
        let formatted = serialize(&document);
        assert!(formatted.contains("values = [1, 2,]"), "{formatted}");
        assert_eq!(serialize(&parse(&formatted)), formatted);
    }

    #[test]
    fn raw_literal_completeness_handles_escaped_quotes() {
        assert!(raw_literal_is_complete("'abc'", '\''));
        assert!(!raw_literal_is_complete("'abc\\'", '\''));
        assert!(raw_literal_is_complete("'abc\\\\'", '\''));
        assert!(!raw_literal_is_complete("'abc", '\''));
    }

    #[test]
    fn rebuilds_unterminated_strings_with_escaped_quotes() {
        let document = parse("[Block]\n  label = 'abc\\'\n[]\n");
        assert!(!document.diagnostics.is_empty());
        let formatted = serialize(&document);
        let reparsed = parse(&formatted);
        assert!(
            reparsed.diagnostics.is_empty(),
            "formatter did not repair the literal: {:?}\n{formatted}",
            reparsed.diagnostics
        );
        assert_eq!(serialize(&reparsed), formatted);
    }
}
