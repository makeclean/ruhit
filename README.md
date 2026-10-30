# ruhit

A resilient, span-aware Rust parser, AST and formatter for the
[MOOSE](https://mooseframework.inl.gov/) **Hierarchical Input Text (HIT)**
format, designed to compile to WebAssembly for browser-based input editors.

The parser is a hand-written lexer + recursive-descent parser. It performs a
single forward pass, never panics on malformed input, and records recoverable
errors as structured diagnostics so an editor can keep working while the user
types.

## Highlights

- **Lossless-oriented AST** — parameters, blocks, comments, numbers, strings,
  arrays and dot-prefixed section paths retain their source spelling where it
  matters for formatting.
- **Spans on every node** — 1-based line/column (Unicode scalar values) plus
  0-based byte offsets.
- **Resilient parsing** — missing `=`, missing values, unclosed headers,
  unclosed blocks, unclosed arrays and unterminated strings produce diagnostics
  instead of panics.
- **Comment preservation** — inline and block comments stay in source order,
  including comments inside multiline arrays.
- **Clean formatting** — deterministic 2-space indentation by default,
  configurable via `FormatOptions`.
- **WASM bindings** — `parse_hit`, `serialize_hit` and `validate_hit` exchange
  plain JSON through `serde_json`.
- **Corpus harness** — drop MOOSE `.i` decks into `tests/corpus/` and they are
  parsed, validated, formatted and re-parsed automatically.

## Project layout

```text
src/
  ast.rs          Serializable AST, spans, diagnostics, helpers
  lexer.rs        Tokenizer (tokens, strings, comments, escapes)
  parser.rs       Recursive-descent parser + error recovery + linting
  serializer.rs   AST-to-HIT formatter
  lib.rs          Public API and wasm-bindgen exports
tests/
  corpus/         MOOSE .i input decks (valid and invalid)
  corpus.rs       Corpus ingestion harness
  json.rs         Serde JSON contract / round-trip tests
bindings/
  ruhit.ts        Typed TypeScript facade for the generated WASM package
```

## Native usage

```toml
[dependencies]
ruhit = "0.1"
```

```rust
use ruhit::{parse, serialize, validate};

let source = r#"
[Mesh]
  type = GeneratedMesh
  dim = 2
  nx = 100
[]
"#;

let document = parse(source);
assert!(document.diagnostics.is_empty());

let mesh = document.block("Mesh").unwrap();
assert_eq!(mesh.block_type.as_deref(), Some("GeneratedMesh"));
assert_eq!(mesh.parameter("nx").unwrap().value.as_f64(), Some(100.0));

let formatted = serialize(&document);
assert_eq!(formatted, "[Mesh]\n  type = GeneratedMesh\n  dim = 2\n  nx = 100\n[]\n");

// Lint/syntax diagnostics only.
let diagnostics = validate(source);
assert!(diagnostics.is_empty());
```

### Formatting options

```rust
use ruhit::{parse, serialize_with_options, FormatOptions};

let document = parse("[Block]\n a = 1\n[]\n");
let text = serialize_with_options(
    &document,
    &FormatOptions {
        indent_width: 4,
        final_newline: false,
    },
);
assert_eq!(text, "[Block]\n    a = 1\n[]");
```

## WASM / TypeScript usage

The `wasm` feature enables the `wasm-bindgen` layer. The crate already declares
`crate-type = ["cdylib", "rlib"]`.

With `wasm-pack`:

```bash
rustup target add wasm32-unknown-unknown
wasm-pack build --target web --features wasm
```

Or with `wasm-bindgen` directly:

```bash
rustup target add wasm32-unknown-unknown
cargo build --target wasm32-unknown-unknown --release --features wasm
wasm-bindgen \
  target/wasm32-unknown-unknown/release/ruhit.wasm \
  --out-dir pkg \
  --target web
```

`wasm-bindgen` generates the JavaScript glue and `.d.ts` declarations. Typical
usage:

```ts
import init, { parse_hit, serialize_hit, validate_hit } from "./pkg/ruhit.js";

await init();

const source = "[Mesh]\n  type = GeneratedMesh\n  nx = 100\n[]\n";

const ast = parse_hit(source);
if (ast.diagnostics.some((d: { severity: string }) => d.severity === "error")) {
  // The AST is still usable for partial editing.
  console.warn(ast.diagnostics);
}

const text = serialize_hit(ast);
const diagnostics = validate_hit(source);

for (const diagnostic of diagnostics) {
  console.log(
    `${diagnostic.severity} ${diagnostic.code} ` +
      `at ${diagnostic.line}:${diagnostic.column}: ${diagnostic.message}`,
  );
}
```

A typed TypeScript facade with AST/diagnostic interfaces and camelCase wrappers
(`parseHit`, `serializeHit`, `validateHit`) is provided in
[`bindings/ruhit.ts`](bindings/ruhit.ts). It assumes the generated package is in
`pkg/`; adjust the import path if your frontend uses a different output
directory.

```ts
import { loadRuhit, hasErrors } from "./bindings/ruhit";

const ruhit = await loadRuhit();
const ast = ruhit.parseHit(source);
if (hasErrors(ast)) {
  console.warn(ast.diagnostics);
}
const formatted = ruhit.serializeHit(ast);
```

### Exported functions

| Function | Description |
| --- | --- |
| `parse_hit(content: string): any` | Returns a JSON AST object. Recoverable errors are in `ast.diagnostics`. Throws only if JSON encoding fails. |
| `serialize_hit(ast: any): string` | Converts a JSON AST back into `.i` text. |
| `validate_hit(content: string): Diagnostic[]` | Returns syntax errors and lint warnings with line/column coordinates. |

## JSON AST

The AST is serializable with Serde and uses camelCase field names. Every node
has a `span`:

```json
{
  "start": { "line": 1, "column": 1, "offset": 0 },
  "end": { "line": 1, "column": 7, "offset": 6 }
}
```

- `line` and `column` are **1-based**.
- Columns count Unicode scalar values (`char`s), not bytes or UTF-16 units.
- `offset` is a **0-based byte offset**.
- `end` is exclusive.

A representative document:

```json
{
  "items": [
    {
      "kind": "block",
      "name": "Mesh",
      "path": null,
      "rawHeader": "Mesh",
      "blockType": "GeneratedMesh",
      "items": [
        {
          "kind": "parameter",
          "key": "type",
          "value": {
            "kind": "string",
            "value": "GeneratedMesh",
            "raw": "GeneratedMesh",
            "quote": null,
            "span": { "start": { "line": 2, "column": 10, "offset": 16 },
                      "end":   { "line": 2, "column": 23, "offset": 29 } }
          },
          "span": { "start": { "line": 2, "column": 3, "offset": 9 },
                    "end":   { "line": 2, "column": 23, "offset": 29 } },
          "keySpan": { "start": { "line": 2, "column": 3, "offset": 9 },
                       "end":   { "line": 2, "column": 7, "offset": 13 } },
          "valueSpan": { "start": { "line": 2, "column": 10, "offset": 16 },
                         "end":   { "line": 2, "column": 23, "offset": 29 } }
        }
      ],
      "span": { "start": { "line": 1, "column": 1, "offset": 0 },
                "end":   { "line": 4, "column": 3, "offset": 43 } },
      "headerSpan": { "start": { "line": 1, "column": 1, "offset": 0 },
                      "end":   { "line": 1, "column": 7, "offset": 6 } },
      "closing": "",
      "closeSpan": { "start": { "line": 4, "column": 1, "offset": 41 },
                     "end":   { "line": 4, "column": 3, "offset": 43 } }
    }
  ],
  "span": { "start": { "line": 1, "column": 1, "offset": 0 },
            "end":   { "line": 4, "column": 3, "offset": 43 } },
  "diagnostics": []
}
```

### Node shapes

| Node | Fields |
| --- | --- |
| `Document` | `items`, `span`, `diagnostics` |
| `Item` | Internally tagged with `kind`: `parameter`, `block` or `comment` |
| `Parameter` | `key`, `value`, `span`, `keySpan`, `valueSpan` |
| `Block` | `name`, `path`, `rawHeader`, `blockType`, `items`, `span`, `headerSpan`, `closing`, `closeSpan` |
| `Comment` | `text` (without `#`), `inline`, `span` |
| `Value` | Flattened `kind`: `string`, `number`, `boolean`, `array` or `missing`, plus `span` |
| `ArrayElement` | Externally tagged: `{"value": Value}` or `{"comment": Comment}` |

`Block.blockType` is a convenience mirror of a direct `type = ...` parameter.
Dot-prefixed headers such as `[./my_mesh]` are preserved in `rawHeader` and
`path`, with `name` set to the final component (`my_mesh`).

### Value examples

```json
{ "kind": "string", "value": "hello world", "raw": "'hello world'", "quote": "'", "span": {} }
{ "kind": "number", "raw": "1.5e-3", "value": 0.0015, "integer": false, "span": {} }
{ "kind": "boolean", "value": true, "span": {} }
{ "kind": "array", "elements": [{ "value": { "kind": "number", "raw": "1", "value": 1.0, "integer": true, "span": {} } }], "multiline": false, "trailingComma": false, "span": {} }
{ "kind": "missing", "span": {} }
```

A minimal AST for `serialize_hit` may omit spans and source spellings. For
example, this is enough to produce `greeting = 'hello world'`:

```json
{
  "items": [
    {
      "kind": "parameter",
      "key": "greeting",
      "value": { "kind": "string", "value": "hello world" }
    }
  ]
}
```

> **Editing note:** `raw` on string/number values and `rawHeader` on blocks are
> the original source spellings. The formatter uses them when present to keep
> escapes and numeric notation lossless. A frontend that mutates `value`,
> `name` or `path` should clear or update the corresponding raw field, otherwise
> the old spelling is written back.

## Supported HIT syntax

- Sections: `[Mesh]`, `[./subblock]`, `[Mesh/Sub]`, closing headers `[]` and
  `[../]`.
- Parameters: `key = value`, with quoted or bare keys.
- Strings: single- and double-quoted, with `\\`, `\'`, `\"`, `\n`, `\t`, `\r`,
  `\0` escapes and multiline strings.
- Numbers: integers, decimals, leading `+`/`-`, `.5`, `1.`, and scientific
  notation such as `1.5e-3` / `2E11`.
- Booleans: `true`/`false` (case-insensitive, normalized on output).
- Vectors: quoted `'a b c'`, unbracketed `1 2 3` or `1, 2, 3`, bracket arrays
  `[1, 2, 3]`, multiline arrays, nested arrays (including empty arrays such as
  `[]` written as their own element), trailing commas, and comments between
  array elements.
- Comments: `# ...` at top level, inside blocks, inline after a value, and
  inside arrays.

## Error recovery and diagnostics

The parser synchronizes at the next newline, block boundary or closing bracket.
Recoverable errors still produce an AST, with `ValueKind::Missing` placeholders
where a value could not be parsed. Unclosed blocks are closed implicitly by the
formatter.

| Code | Severity | Meaning |
| --- | --- | --- |
| `missing-equals` | error | A parameter name was not followed by `=` |
| `missing-value` | error | `=` was not followed by a value |
| `unclosed-block-header` | error | A section header was missing `]` |
| `unclosed-block` | error | A block reached EOF without a closing header |
| `unclosed-array` | error | An array was missing `]` |
| `unterminated-string` | error | A quoted string reached EOF |
| `unexpected-closing-block` | error | A closing header appeared at top level |
| `unexpected-token` | error | Stray punctuation or invalid syntax |
| `nesting-too-deep` | error | Block or array nesting exceeded the safety limit (128) |
| `unknown-escape` | warning | Unknown backslash escape preserved verbatim |
| `duplicate-parameter` | warning | The same key appears twice in one block |

## Testing

```bash
cargo test --all-targets
cargo test --all-targets --features wasm
cargo check --no-default-features
```

The corpus harness reads every `.i` file in `tests/corpus/`. Files whose names
start with `invalid_` are expected to produce errors; all other files must parse
cleanly, format idempotently, and preserve their document structure after a
round trip. Add new decks directly to the directory — no registration needed.

## Feature flags

| Feature | Default | Description |
| --- | --- | --- |
| `serde` | yes | Derives `Serialize`/`Deserialize` for the AST |
| `wasm` | no | Enables `wasm-bindgen` exports and the JSON bridge |

## License

MIT OR Apache-2.0.
