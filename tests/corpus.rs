//! Corpus harness.
//!
//! Every `.i` file under `tests/corpus/` is parsed, validated, formatted and
//! re-parsed. Files whose name starts with `invalid_` are expected to produce
//! at least one error diagnostic; all other files are expected to be clean.
//!
//! Drop additional MOOSE input decks into the directory to expand coverage.
//! No registration is required.

use std::fs;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};

use ruhit::{
    parse, serialize, validate, ArrayElement, Block, Document, Item, Severity, Span, Value,
    ValueKind,
};

#[derive(Debug, Default, PartialEq, Eq)]
struct Counts {
    parameters: usize,
    blocks: usize,
    comments: usize,
    array_values: usize,
}

fn corpus_directory() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/corpus")
}

fn corpus_files() -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = fs::read_dir(corpus_directory())
        .expect("read tests/corpus")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "i"))
        .collect();
    files.sort();
    files
}

#[test]
fn corpus_parses_formats_and_reparses() {
    let files = corpus_files();
    assert!(!files.is_empty(), "expected at least one corpus file");

    for path in files {
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("<unknown>");
        let source = fs::read_to_string(&path).expect("read corpus file");
        let invalid = file_name.starts_with("invalid_");

        let parsed = catch_unwind(AssertUnwindSafe(|| parse(&source)))
            .unwrap_or_else(|_| panic!("parser panicked on {file_name}"));

        assert_document_spans(&parsed, file_name);

        let validated = validate(&source);
        assert_eq!(
            validated, parsed.diagnostics,
            "validate() and parse() disagree for {file_name}"
        );

        if invalid {
            assert!(
                parsed.has_errors(),
                "expected error diagnostics for {file_name}, got {:?}",
                parsed.diagnostics
            );
            assert!(
                parsed
                    .diagnostics
                    .iter()
                    .any(|diagnostic| diagnostic.severity == Severity::Error),
                "expected at least one error severity for {file_name}"
            );
        } else {
            assert!(
                parsed.diagnostics.is_empty(),
                "unexpected diagnostics for {file_name}: {:?}",
                parsed.diagnostics
            );
        }

        let formatted = serialize(&parsed);
        let reparsed = catch_unwind(AssertUnwindSafe(|| parse(&formatted)))
            .unwrap_or_else(|_| panic!("parser panicked on formatted {file_name}"));

        if !invalid {
            assert!(
                reparsed.diagnostics.is_empty(),
                "formatter produced invalid HIT for {file_name}: {:?}\n---\n{formatted}",
                reparsed.diagnostics
            );
            assert_eq!(
                counts(&parsed.items),
                counts(&reparsed.items),
                "round-trip changed the document structure for {file_name}"
            );
        }

        let reformatted = serialize(&reparsed);
        assert_eq!(
            formatted, reformatted,
            "formatting is not idempotent for {file_name}"
        );
    }
}

#[test]
fn corpus_contains_both_valid_and_invalid_decks() {
    let files = corpus_files();
    assert!(
        files.iter().any(|path| !path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("")
            .starts_with("invalid_")),
        "expected at least one valid corpus file"
    );
    assert!(
        files.iter().any(|path| path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("")
            .starts_with("invalid_")),
        "expected at least one invalid corpus file"
    );
}

fn counts(items: &[Item]) -> Counts {
    let mut counts = Counts::default();
    for item in items {
        match item {
            Item::Parameter(parameter) => {
                counts.parameters += 1;
                count_value(&parameter.value, &mut counts);
            }
            Item::Block(block) => {
                counts.blocks += 1;
                let nested = counts_of_block(block);
                counts.parameters += nested.parameters;
                counts.blocks += nested.blocks;
                counts.comments += nested.comments;
                counts.array_values += nested.array_values;
            }
            Item::Comment(_) => counts.comments += 1,
        }
    }
    counts
}

fn counts_of_block(block: &Block) -> Counts {
    counts(&block.items)
}

fn count_value(value: &Value, counts: &mut Counts) {
    if let ValueKind::Array { elements, .. } = &value.kind {
        for element in elements {
            match element {
                ArrayElement::Value(value) => {
                    counts.array_values += 1;
                    count_value(value, counts);
                }
                ArrayElement::Comment(_) => counts.comments += 1,
            }
        }
    }
}

fn assert_document_spans(document: &Document, file_name: &str) {
    if !document.items.is_empty() {
        check_span(document.span, file_name, "document");
    }
    for item in &document.items {
        check_item_spans(item, file_name);
    }
    for diagnostic in &document.diagnostics {
        check_span(diagnostic.span, file_name, "diagnostic");
        assert!(diagnostic.line >= 1, "{file_name}: diagnostic line");
        assert!(diagnostic.column >= 1, "{file_name}: diagnostic column");
    }
}

fn check_item_spans(item: &Item, file_name: &str) {
    check_span(item.span(), file_name, "item");
    match item {
        Item::Parameter(parameter) => {
            check_span(parameter.key_span, file_name, "parameter key");
            check_span(parameter.value_span, file_name, "parameter value");
            check_value_spans(&parameter.value, file_name);
        }
        Item::Block(block) => {
            check_span(block.header_span, file_name, "block header");
            if let Some(close_span) = block.close_span {
                check_span(close_span, file_name, "block close");
            }
            for child in &block.items {
                check_item_spans(child, file_name);
            }
        }
        Item::Comment(comment) => {
            check_span(comment.span, file_name, "comment");
        }
    }
}

fn check_value_spans(value: &Value, file_name: &str) {
    check_span(value.span, file_name, "value");
    if let ValueKind::Array { elements, .. } = &value.kind {
        for element in elements {
            check_span(element.span(), file_name, "array element");
            match element {
                ArrayElement::Value(value) => check_value_spans(value, file_name),
                ArrayElement::Comment(comment) => {
                    check_span(comment.span, file_name, "array comment");
                }
            }
        }
    }
}

fn check_span(span: Span, file_name: &str, context: &str) {
    assert!(
        span.start.line >= 1 && span.start.column >= 1,
        "{file_name}: {context} has an invalid start position: {span:?}"
    );
    assert!(
        span.end.line >= 1 && span.end.column >= 1,
        "{file_name}: {context} has an invalid end position: {span:?}"
    );
    assert!(
        span.start.offset <= span.end.offset,
        "{file_name}: {context} start offset is after end offset: {span:?}"
    );
    assert!(
        span.start.line <= span.end.line,
        "{file_name}: {context} start line is after end line: {span:?}"
    );
    if span.start.line == span.end.line {
        assert!(
            span.start.column <= span.end.column,
            "{file_name}: {context} start column is after end column: {span:?}"
        );
    }
}
