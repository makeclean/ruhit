//! Shared support for the WASP conformance harness.
//!
//! The manifest is the single source of truth for the 1:1 mapping between
//! WASP's tests and ruhit's tests.  The projection below is deliberately
//! semantic: WASP's decorative tree nodes are not part of the comparison.

#![allow(dead_code)]

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::OnceLock;

use ruhit::ast::{ArrayElement, Block, Document, Item, Parameter, Value, ValueKind};
use ruhit::{parse_with_options, ParseOptions};
use serde::Deserialize;
use serde_json::{json, Value as JsonValue};

#[derive(Debug, Deserialize)]
pub struct Manifest {
    pub version: u32,
    pub wasp_repo: String,
    pub wasp_commit: String,
    pub moose_vendor_path: String,
    #[serde(rename = "case", default)]
    pub cases: Vec<Case>,
}

#[derive(Debug, Deserialize)]
pub struct Case {
    pub id: String,
    pub suite: String,
    pub wasp_test: String,
    pub rust_test: String,
    pub category: String,
    pub phase: String,
    pub status: String,
    #[serde(default)]
    pub input: Option<String>,
    #[serde(default)]
    pub workdir: Option<String>,
    #[serde(default)]
    pub fixtures: Vec<String>,
    #[serde(default)]
    pub expect: Option<String>,
    #[serde(default)]
    pub schema: Option<String>,
    #[serde(default)]
    pub fail_input: Option<String>,
    #[serde(default)]
    pub pass_input: Option<String>,
    #[serde(default)]
    pub expected_errors: Option<String>,
}

impl Case {
    pub fn name(&self) -> &str {
        self.id.split_once('/').map(|(_, name)| name).unwrap_or(&self.id)
    }

    fn expected_projection(&self) -> PathBuf {
        wasp_root()
            .join("expected")
            .join(&self.suite)
            .join(format!("{}.json", self.name()))
    }

    fn divergence_snapshot(&self) -> PathBuf {
        wasp_root()
            .join("divergences")
            .join(&self.suite)
            .join(format!("{}.json", self.name()))
    }
}

static MANIFEST: OnceLock<Manifest> = OnceLock::new();

pub fn wasp_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/wasp")
}

pub fn manifest() -> &'static Manifest {
    MANIFEST.get_or_init(|| {
        let path = wasp_root().join("manifest.toml");
        let text = fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("could not read {}: {error}", path.display()));
        toml::from_str(&text)
            .unwrap_or_else(|error| panic!("could not parse {}: {error}", path.display()))
    })
}

pub fn case(id: &str) -> &'static Case {
    manifest()
        .cases
        .iter()
        .find(|case| case.id == id)
        .unwrap_or_else(|| panic!("no WASP conformance case named {id}"))
}

fn update_mode() -> bool {
    std::env::var_os("WASP_UPDATE").is_some()
}

// ---------------------------------------------------------------------------
// Temporary fixture directories
// ---------------------------------------------------------------------------

static TEMP_COUNTER: AtomicUsize = AtomicUsize::new(0);

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "ruhit-wasp-{}-{}",
            std::process::id(),
            TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).expect("create WASP fixture temp dir");
        Self { path }
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn copy_dir_files(from: &Path, to: &Path) {
    let entries = fs::read_dir(from)
        .unwrap_or_else(|error| panic!("could not read fixture dir {}: {error}", from.display()));
    for entry in entries {
        let entry = entry.expect("read fixture entry");
        if entry.file_type().map(|kind| kind.is_file()).unwrap_or(false) {
            fs::copy(entry.path(), to.join(entry.file_name())).unwrap_or_else(|error| {
                panic!(
                    "could not copy fixture {} to {}: {error}",
                    entry.path().display(),
                    to.display()
                )
            });
        }
    }
}

/// Materialize a case's fixtures into a private directory.  Returns the source
/// text and the temporary directory guard (which must stay alive while parsing).
fn prepare_hit_source(case: &Case) -> (String, Option<TempDir>) {
    let input = case
        .input
        .as_ref()
        .unwrap_or_else(|| panic!("HIT case {} has no input", case.id));
    let input_path = wasp_root().join(input);

    if let Some(workdir) = &case.workdir {
        let temp = TempDir::new();
        copy_dir_files(&wasp_root().join(workdir), &temp.path);
        let local = temp.path.join(input_path.file_name().expect("input file name"));
        if !local.exists() {
            fs::copy(&input_path, &local).unwrap_or_else(|error| {
                panic!(
                    "could not copy input {} to {}: {error}",
                    input_path.display(),
                    local.display()
                )
            });
        }
        let source = fs::read_to_string(&local)
            .unwrap_or_else(|error| panic!("could not read {}: {error}", local.display()));
        (source, Some(temp))
    } else {
        let source = fs::read_to_string(&input_path)
            .unwrap_or_else(|error| panic!("could not read {}: {error}", input_path.display()));
        (source, None)
    }
}

// ---------------------------------------------------------------------------
// HIT projection
// ---------------------------------------------------------------------------

fn split_components(path: &str) -> Vec<&str> {
    path.split('/').filter(|component| !component.is_empty()).collect()
}

fn join_path(base: &str, name: &str) -> String {
    let mut parts = split_components(base);
    parts.extend(split_components(name));
    format!("/{}", parts.join("/"))
}

fn expand_path(base: &str, raw: &str) -> String {
    let mut parts: Vec<String> = split_components(base)
        .into_iter()
        .map(str::to_string)
        .collect();
    for component in raw.trim().split('/') {
        match component {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            other => parts.push(other.to_string()),
        }
    }
    format!("/{}", parts.join("/"))
}

fn last_component(path: &str) -> String {
    split_components(path)
        .last()
        .copied()
        .unwrap_or_default()
        .to_string()
}

fn implicit_prefixes(
    base: &str,
    path: &str,
    line: u32,
    column: u32,
    include_final: bool,
    nodes: &mut Vec<JsonValue>,
) {
    let base_parts = split_components(base);
    let path_parts = split_components(path);
    if path_parts.len() <= base_parts.len() {
        return;
    }
    let end = if include_final {
        path_parts.len()
    } else {
        path_parts.len().saturating_sub(1)
    };
    for index in (base_parts.len() + 1)..=end {
        let prefix = format!("/{}", path_parts[..index].join("/"));
        let name = path_parts[index - 1];
        nodes.push(json!({
            "kind": "block",
            "path": prefix,
            "name": name,
            "header": name,
            "line": line,
            "column": column,
        }));
    }
}

fn scalar_projection(value: &Value) -> (String, &'static str) {
    match &value.kind {
        ValueKind::String { raw, value, quote } => {
            if quote.is_some() {
                (raw.clone(), "quoted-string")
            } else {
                (value.clone(), "string")
            }
        }
        ValueKind::Number {
            raw,
            integer,
            value: _,
        } => (raw.clone(), if *integer { "integer" } else { "real" }),
        ValueKind::Boolean { value } => (value.to_string(), "string"),
        ValueKind::Missing => (String::new(), "missing"),
        ValueKind::Array { .. } => (String::new(), "array"),
    }
}

fn array_parts(elements: &[ArrayElement]) -> Vec<JsonValue> {
    elements
        .iter()
        .map(|element| match element {
            ArrayElement::Value(value) => {
                let (text, kind) = scalar_projection(value);
                json!({"partType": "value", "value": text, "valueType": kind})
            }
            ArrayElement::Comment(comment) => {
                json!({"partType": "comment", "value": format!("#{}", comment.text)})
            }
            ArrayElement::Quote(quote) => {
                json!({"partType": "quote", "value": quote.mark.to_string()})
            }
            ArrayElement::Semicolon(_) => {
                json!({"partType": "semicolon", "value": ";"})
            }
        })
        .collect()
}

fn project_parameter(parameter: &Parameter, base: &str, nodes: &mut Vec<JsonValue>) {
    if parameter.key == "!include" {
        let target = parameter.value.as_str().unwrap_or_default();
        nodes.push(json!({
            "kind": "include",
            "path": join_path(base, "incl"),
            "name": "incl",
            "text": format!("!include {target}"),
            "target": target,
            "line": parameter.span.start.line,
            "column": parameter.span.start.column,
        }));
        return;
    }

    let path = expand_path(base, &parameter.key);
    implicit_prefixes(
        base,
        &path,
        parameter.span.start.line,
        parameter.span.start.column,
        false,
        nodes,
    );

    let line = parameter.span.start.line;
    let column = parameter.span.start.column;
    if let ValueKind::Array { elements, .. } = &parameter.value.kind {
        nodes.push(json!({
            "kind": "array",
            "path": path,
            "name": last_component(&path),
            "assign": parameter.assign.as_str(),
            "override": parameter.override_assign,
            "parts": array_parts(elements),
            "line": line,
            "column": column,
        }));
    } else if parameter.value.is_missing() {
        nodes.push(json!({
            "kind": "parameter",
            "path": path,
            "name": last_component(&path),
            "assign": parameter.assign.as_str(),
            "override": parameter.override_assign,
            "values": [],
            "line": line,
            "column": column,
        }));
    } else {
        let (value, value_type) = scalar_projection(&parameter.value);
        nodes.push(json!({
            "kind": "parameter",
            "path": path,
            "name": last_component(&path),
            "assign": parameter.assign.as_str(),
            "override": parameter.override_assign,
            "values": [{"value": value, "valueType": value_type}],
            "value": value,
            "valueType": value_type,
            "line": line,
            "column": column,
        }));
    }
}

fn project_block(block: &Block, base: &str, nodes: &mut Vec<JsonValue>) {
    let path = expand_path(base, block.header());
    let before = nodes.len();
    implicit_prefixes(
        base,
        &path,
        block.span.start.line,
        block.span.start.column,
        true,
        nodes,
    );

    // WASP reports `[./fred]` with the declarator text `fred`, while other
    // path spellings keep their slashes.  Mirror that in the projection.
    let header = block.header().strip_prefix("./").unwrap_or(block.header());

    // Replace the final implicit entry with the block's real header.
    if let Some(last) = nodes.last_mut() {
        if last.get("kind").and_then(|kind| kind.as_str()) == Some("block")
            && last.get("path").and_then(|path| path.as_str()) == Some(path.as_str())
        {
            last["header"] = json!(header);
        }
    }

    // An empty-named block has path `/`, which produces no implicit prefix.
    // WASP still records the block node itself.
    if nodes.len() == before {
        nodes.push(json!({
            "kind": "block",
            "path": path,
            "name": block.name,
            "header": header,
            "line": block.span.start.line,
            "column": block.span.start.column,
        }));
    }

    for item in &block.items {
        project_item(item, &path, nodes);
    }
}

fn project_item(item: &Item, base: &str, nodes: &mut Vec<JsonValue>) {
    match item {
        Item::Parameter(parameter) => project_parameter(parameter, base, nodes),
        Item::Block(block) => project_block(block, base, nodes),
        Item::Comment(comment) => nodes.push(json!({
            "kind": "comment",
            "path": join_path(base, "comment"),
            "text": format!("#{}", comment.text),
            "line": comment.span.start.line,
            "column": comment.span.start.column,
        })),
    }
}

fn project_hit(document: &Document) -> JsonValue {
    let mut nodes = vec![json!({"kind": "document", "path": "/"})];
    for item in &document.items {
        project_item(item, "", &mut nodes);
    }
    let diagnostics = document
        .diagnostics
        .iter()
        .map(|diagnostic| {
            json!({
                "source": "stream input",
                "line": diagnostic.span.start.line,
                "column": diagnostic.span.start.column,
                "message": diagnostic.message,
            })
        })
        .collect::<Vec<_>>();
    json!({
        "suite": "hit",
        "parse": !document.has_errors(),
        "diagnostics": diagnostics,
        "nodes": nodes,
    })
}

// ---------------------------------------------------------------------------
// HIVE projection (placeholder until the validator is ported)
// ---------------------------------------------------------------------------

fn project_hive(case: &Case) -> JsonValue {
    // Parsing the HIT inputs is already meaningful and keeps the fixture
    // plumbing honest.  The validation engine itself is ported in phase D.
    let fail_source = case
        .fail_input
        .as_ref()
        .and_then(|path| fs::read_to_string(wasp_root().join(path)).ok())
        .unwrap_or_default();
    let pass_source = case
        .pass_input
        .as_ref()
        .and_then(|path| fs::read_to_string(wasp_root().join(path)).ok())
        .unwrap_or_default();
    let fail_document = parse_with_options(&fail_source, ParseOptions::wasp());
    let pass_document = parse_with_options(&pass_source, ParseOptions::wasp());
    let combined = "ruhit HIVE validation is not implemented yet\n".to_string();
    json!({
        "suite": "hive",
        "valid": false,
        "failParse": !fail_document.has_errors(),
        "passParse": !pass_document.has_errors(),
        "combined": combined,
        "errors": ["ruhit HIVE validation is not implemented yet"],
    })
}

// ---------------------------------------------------------------------------
// Running and comparing cases
// ---------------------------------------------------------------------------

fn read_json(path: &Path) -> JsonValue {
    let text = fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("could not read {}: {error}", path.display()));
    serde_json::from_str(&text)
        .unwrap_or_else(|error| panic!("could not parse {}: {error}", path.display()))
}

fn write_json(path: &Path, value: &JsonValue) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .unwrap_or_else(|error| panic!("could not create {}: {error}", parent.display()));
    }
    let text = serde_json::to_string_pretty(value).expect("serialize projection");
    fs::write(path, format!("{text}\n"))
        .unwrap_or_else(|error| panic!("could not write {}: {error}", path.display()));
}

fn first_difference(expected: &JsonValue, actual: &JsonValue, path: &str) -> Option<String> {
    match (expected, actual) {
        (JsonValue::Object(left), JsonValue::Object(right)) => {
            let keys: BTreeSet<&String> = left.keys().chain(right.keys()).collect();
            for key in keys {
                let child = format!("{path}.{key}");
                match (left.get(key), right.get(key)) {
                    (Some(left), Some(right)) => {
                        if let Some(difference) = first_difference(left, right, &child) {
                            return Some(difference);
                        }
                    }
                    (Some(left), None) => {
                        return Some(format!(
                            "{child}: expected {left}, actual <missing>"
                        ));
                    }
                    (None, Some(right)) => {
                        return Some(format!(
                            "{child}: expected <missing>, actual {right}"
                        ));
                    }
                    (None, None) => {}
                }
            }
            None
        }
        (JsonValue::Array(left), JsonValue::Array(right)) => {
            if left.len() != right.len() {
                return Some(format!(
                    "{path}: expected {} array elements, actual {}",
                    left.len(),
                    right.len()
                ));
            }
            for (index, (left, right)) in left.iter().zip(right.iter()).enumerate() {
                if let Some(difference) =
                    first_difference(left, right, &format!("{path}[{index}]"))
                {
                    return Some(difference);
                }
            }
            None
        }
        (left, right) if left == right => None,
        (left, right) => Some(format!("{path}: expected {left}, actual {right}")),
    }
}

fn panic_with_difference(label: &str, expected: &JsonValue, actual: &JsonValue) -> ! {
    let difference = first_difference(expected, actual, "$")
        .unwrap_or_else(|| "values differ".to_string());
    panic!(
        "{label}\nfirst difference: {difference}\n\n--- expected ---\n{}\n\n--- actual ---\n{}",
        serde_json::to_string_pretty(expected).unwrap_or_default(),
        serde_json::to_string_pretty(actual).unwrap_or_default()
    );
}

fn actual_for(case: &Case) -> JsonValue {
    match case.suite.as_str() {
        "hit" => {
            let (source, _temp) = prepare_hit_source(case);
            let document = parse_with_options(&source, ParseOptions::wasp());
            // The accept/reject decision is part of the projection (`parse`).
            // Do not assert it here: a baseline case may legitimately diverge.
            project_hit(&document)
        }
        "hive" => project_hive(case),
        other => panic!("unknown suite {other} in case {}", case.id),
    }
}

pub fn run_case(id: &str) {
    let case = case(id);
    let actual = actual_for(case);
    let expected = read_json(&case.expected_projection());

    if case.status == "match" {
        if actual != expected {
            panic_with_difference(
                &format!("{} diverged from the WASP oracle", case.id),
                &expected,
                &actual,
            );
        }
        return;
    }

    // Ratchet: a case that is marked as a known divergence must not silently
    // become conformant.  When it does, flip the manifest status to "match".
    // During an explicit baseline update we allow the caller to discover the
    // newly matching cases and update the manifest in one pass.
    if actual == expected {
        if update_mode() {
            return;
        }
        panic!(
            "{} now matches the WASP oracle; update tests/wasp/manifest.toml status from {:?} to \"match\"",
            case.id, case.status
        );
    }

    let snapshot_path = case.divergence_snapshot();
    if update_mode() {
        write_json(&snapshot_path, &actual);
    } else if !snapshot_path.exists() {
        panic!(
            "missing divergence snapshot {}; run `WASP_UPDATE=1 cargo test --test wasp_conformance`",
            snapshot_path.display()
        );
    }
    let snapshot = read_json(&snapshot_path);
    if actual != snapshot {
        panic_with_difference(
            &format!(
                "{} changed since its known-divergence snapshot was recorded",
                case.id
            ),
            &snapshot,
            &actual,
        );
    }
}

pub fn assert_manifest(generated_ids: &[&str]) {
    let manifest = manifest();
    assert_eq!(manifest.version, 1, "unsupported manifest version");
    assert!(!manifest.wasp_commit.is_empty());
    assert_eq!(
        manifest.cases.len(),
        116,
        "the WASP conformance manifest must contain 95 HIT + 21 HIVE cases"
    );

    let hit_count = manifest.cases.iter().filter(|case| case.suite == "hit").count();
    let hive_count = manifest.cases.iter().filter(|case| case.suite == "hive").count();
    assert_eq!(hit_count, 95, "expected 95 HIT cases");
    assert_eq!(hive_count, 21, "expected 21 HIVE cases");

    let manifest_ids: BTreeSet<&str> = manifest.cases.iter().map(|case| case.id.as_str()).collect();
    let generated: BTreeSet<&str> = generated_ids.iter().copied().collect();
    assert_eq!(
        manifest_ids, generated,
        "build.rs generated tests do not match the manifest 1:1"
    );

    let mut rust_names = BTreeSet::new();
    for case in &manifest.cases {
        assert!(
            rust_names.insert(case.rust_test.as_str()),
            "duplicate Rust test name {}",
            case.rust_test
        );
        assert!(
            matches!(
                case.status.as_str(),
                "match" | "baseline" | "known-divergence" | "unsupported"
            ),
            "{} has unknown status {}",
            case.id,
            case.status
        );
        assert_eq!(
            case.wasp_test.split('.').next(),
            Some(if case.suite == "hit" {
                "HITInterpreter"
            } else {
                "HITHIVE"
            }),
            "{} has a WASP test name from the wrong suite",
            case.id
        );

        match case.suite.as_str() {
            "hit" => {
                let input = case.input.as_ref().unwrap_or_else(|| {
                    panic!("{} is missing its input path", case.id)
                });
                assert!(
                    wasp_root().join(input).is_file(),
                    "{} input does not exist: {input}",
                    case.id
                );
                for fixture in &case.fixtures {
                    let path = wasp_root()
                        .join(case.workdir.as_deref().unwrap_or_default())
                        .join(fixture);
                    assert!(
                        path.is_file(),
                        "{} fixture does not exist: {}",
                        case.id,
                        path.display()
                    );
                }
                assert!(
                    matches!(case.expect.as_deref(), Some("ok" | "error")),
                    "{} must declare expect = \"ok\" or \"error\"",
                    case.id
                );
            }
            "hive" => {
                for (label, path) in [
                    ("schema", &case.schema),
                    ("fail_input", &case.fail_input),
                    ("pass_input", &case.pass_input),
                    ("expected_errors", &case.expected_errors),
                ] {
                    let path = path
                        .as_ref()
                        .unwrap_or_else(|| panic!("{} is missing {label}", case.id));
                    assert!(
                        wasp_root().join(path).is_file(),
                        "{} {label} does not exist: {path}",
                        case.id
                    );
                }
            }
            other => panic!("{} has unknown suite {other}", case.id),
        }

        let expected = case.expected_projection();
        assert!(
            expected.is_file(),
            "{} is missing its checked-in WASP oracle projection: {}",
            case.id,
            expected.display()
        );

        if case.status != "match" && !update_mode() {
            let snapshot = case.divergence_snapshot();
            assert!(
                snapshot.is_file(),
                "{} is missing its divergence snapshot: {} (run WASP_UPDATE=1)",
                case.id,
                snapshot.display()
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Reporting
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Match,
    KnownDivergence,
    Unsupported,
    Mismatch,
}

impl Outcome {
    pub fn label(self) -> &'static str {
        match self {
            Outcome::Match => "MATCH",
            Outcome::KnownDivergence => "DIVERGE",
            Outcome::Unsupported => "UNSUPPORTED",
            Outcome::Mismatch => "MISMATCH",
        }
    }
}

pub struct CaseReport {
    pub id: String,
    pub category: String,
    pub phase: String,
    pub status: String,
    pub outcome: Outcome,
    pub detail: Option<String>,
}

pub fn report() -> Vec<CaseReport> {
    manifest()
        .cases
        .iter()
        .map(|case| {
            let actual = actual_for(case);
            let expected = read_json(&case.expected_projection());
            let (outcome, detail) = if actual == expected {
                (Outcome::Match, None)
            } else if case.status == "match" {
                (
                    Outcome::Mismatch,
                    first_difference(&expected, &actual, "$"),
                )
            } else if case.status == "unsupported" {
                (Outcome::Unsupported, None)
            } else {
                (
                    Outcome::KnownDivergence,
                    first_difference(&expected, &actual, "$"),
                )
            };
            CaseReport {
                id: case.id.clone(),
                category: case.category.clone(),
                phase: case.phase.clone(),
                status: case.status.clone(),
                outcome,
                detail,
            }
        })
        .collect()
}
