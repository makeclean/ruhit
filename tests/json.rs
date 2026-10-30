//! JSON shape and Serde round-trip tests. These are the contract used by the
//! WASM/TypeScript layer.

use ruhit::{parse, serialize, Document, Severity};

#[test]
fn ast_round_trips_through_json() {
    let source = r#"# file header
[GlobalParams]
  displacements = 'disp_x disp_y'
[]

[Mesh]
  type = GeneratedMesh
  dim = 2
  nx = 10
  values = [1, 2.5e-3, 3]
  enabled = true
  [./sub]
    label = 'a\'b'
  [../]
[]
"#;

    let document = parse(source);
    assert!(document.diagnostics.is_empty(), "{:?}", document.diagnostics);

    let json = serde_json::to_string_pretty(&document).expect("serialize AST");
    let restored: Document = serde_json::from_str(&json).expect("deserialize AST");

    assert_eq!(document, restored);
    assert_eq!(serialize(&document), serialize(&restored));
}

#[test]
fn json_uses_tagged_items_and_flattened_values() {
    let document = parse("[Block]\n  a = 1\n  b = 'text'\n[]\n");
    let json: serde_json::Value =
        serde_json::to_value(&document).expect("serialize AST as JSON value");

    let block = &json["items"][0];
    assert_eq!(block["kind"], "block");
    assert_eq!(block["name"], "Block");

    let parameter = &block["items"][0];
    assert_eq!(parameter["kind"], "parameter");
    assert_eq!(parameter["key"], "a");
    assert_eq!(parameter["value"]["kind"], "number");
    assert_eq!(parameter["value"]["raw"], "1");
    assert_eq!(parameter["value"]["value"], 1.0);
    assert_eq!(parameter["value"]["integer"], true);
    assert_eq!(parameter["span"]["start"]["line"], 2);
    assert_eq!(parameter["span"]["start"]["column"], 3);

    let string_parameter = &block["items"][1];
    assert_eq!(string_parameter["value"]["kind"], "string");
    assert_eq!(string_parameter["value"]["value"], "text");
    assert_eq!(string_parameter["value"]["quote"], "'");
}

#[test]
fn diagnostics_have_line_and_column() {
    let document = parse("[Block]\n  broken\n[]\n");
    assert_eq!(document.diagnostics.len(), 1);
    let diagnostic = &document.diagnostics[0];
    assert_eq!(diagnostic.severity, Severity::Error);
    assert_eq!(diagnostic.code, "missing-equals");
    assert_eq!(diagnostic.line, 2);
    assert_eq!(diagnostic.column, 3);

    let json = serde_json::to_value(&document.diagnostics).unwrap();
    assert_eq!(json[0]["line"], 2);
    assert_eq!(json[0]["column"], 3);
    assert_eq!(json[0]["span"]["start"]["line"], 2);
}

#[test]
fn accepts_a_minimal_frontend_constructed_ast() {
    let json = r#"{
        "items": [
            {
                "kind": "parameter",
                "key": "greeting",
                "value": { "kind": "string", "value": "hello world" }
            },
            {
                "kind": "block",
                "name": "Mesh",
                "items": [
                    {
                        "kind": "parameter",
                        "key": "dim",
                        "value": { "kind": "number", "value": 2, "raw": "2", "integer": true }
                    },
                    {
                        "kind": "parameter",
                        "key": "enabled",
                        "value": { "kind": "boolean", "value": true }
                    },
                    {
                        "kind": "parameter",
                        "key": "values",
                        "value": {
                            "kind": "array",
                            "elements": [
                                { "value": { "kind": "number", "value": 1, "raw": "1" } },
                                { "value": { "kind": "number", "value": 2, "raw": "2" } }
                            ]
                        }
                    }
                ]
            }
        ]
    }"#;

    let document: Document = serde_json::from_str(json).expect("minimal AST");
    let formatted = serialize(&document);
    assert_eq!(
        formatted,
        "greeting = 'hello world'\n[Mesh]\n  dim = 2\n  enabled = true\n  values = [1, 2]\n[]\n"
    );

    let reparsed = parse(&formatted);
    assert!(reparsed.diagnostics.is_empty(), "{:?}", reparsed.diagnostics);
}

#[test]
fn empty_document_serializes_to_empty_string() {
    let document = parse("");
    assert!(document.items.is_empty());
    assert_eq!(serialize(&document), "");
    let json = serde_json::to_string(&document).unwrap();
    let restored: Document = serde_json::from_str(&json).unwrap();
    assert_eq!(document, restored);
}

#[test]
fn tolerates_empty_span_objects_from_frontends() {
    let json = r#"{
        "items": [
            {
                "kind": "parameter",
                "key": "a",
                "value": { "kind": "number", "value": 1, "span": {} },
                "span": {},
                "keySpan": {},
                "valueSpan": {}
            }
        ],
        "span": {}
    }"#;
    let document: Document = serde_json::from_str(json).expect("empty spans");
    assert_eq!(serialize(&document), "a = 1\n");
}

#[test]
fn non_finite_number_spellings_stay_json_safe() {
    // `1e999` overflows f64. It must not make serde_json fail later; the raw
    // spelling is retained as a string value instead.
    let document = parse("[Block]\n  huge = 1e999\n[]\n");
    assert!(document.diagnostics.is_empty());
    let parameter = document.block("Block").unwrap().parameter("huge").unwrap();
    assert_eq!(parameter.value.as_str(), Some("1e999"));
    let json = serde_json::to_string(&document).expect("JSON-safe AST");
    assert!(json.contains("\"kind\":\"string\""));
}
