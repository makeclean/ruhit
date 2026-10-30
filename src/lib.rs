//! `ruhit` is a resilient parser, AST and formatter for the MOOSE
//! Hierarchical Input Text (HIT) format.
//!
//! The crate is split into four layers:
//!
//! * [`lexer`] — turns source text into spanned tokens.
//! * [`ast`] — serializable, span-aware document model.
//! * [`parser`] — recursive-descent parser with error recovery.
//! * [`serializer`] — AST-to-HIT formatter.
//!
//! With the `wasm` feature enabled, the `wasm` module exposes
//! `parse_hit`, `serialize_hit` and `validate_hit` through `wasm-bindgen`.
//!
//! ```no_run
//! use ruhit::{parse, serialize};
//!
//! let document = parse("[Mesh]\n  type = GeneratedMesh\n[]\n");
//! assert!(document.diagnostics.is_empty());
//! let formatted = serialize(&document);
//! ```

#![forbid(unsafe_code)]

pub mod ast;
pub mod lexer;
pub mod parser;
pub mod serializer;

pub use ast::*;
pub use parser::{parse, parse_with_options, validate, ParseOptions};
pub use serializer::{serialize, serialize_with_options, FormatOptions};

/// WASM bindings. Enabled with the `wasm` feature.
///
/// All functions exchange plain JSON through `serde_json`, so the generated
/// TypeScript declarations use `any` for AST values while the JSON schema is
/// documented in the README. Diagnostics are ordinary JSON objects.
#[cfg(feature = "wasm")]
pub mod wasm {
    use serde::Serialize;
    use wasm_bindgen::prelude::*;

    use crate::{parse, serializer, validate, Document, Diagnostic};

    /// Parse raw HIT text into a JSON AST object.
    ///
    /// Recoverable syntax errors are returned inside the document's
    /// `diagnostics` array; the function only returns `Err` if the AST cannot
    /// be encoded as JSON (which should not happen in practice).
    #[wasm_bindgen]
    pub fn parse_hit(content: &str) -> Result<JsValue, JsError> {
        let document = parse(content);
        to_js_value(&document)
    }

    /// Convert a JSON AST back into MOOSE `.i` input text.
    #[wasm_bindgen]
    pub fn serialize_hit(ast_json: &JsValue) -> Result<String, JsError> {
        let json = js_sys::JSON::stringify(ast_json)
            .map_err(|error| JsError::new(&format!("could not stringify AST: {error:?}")))?;
        let json = json
            .as_string()
            .ok_or_else(|| JsError::new("AST value could not be serialized to JSON"))?;
        let document: Document = serde_json::from_str(&json)
            .map_err(|error| JsError::new(&format!("invalid AST JSON: {error}")))?;
        Ok(serializer::serialize(&document))
    }

    /// Parse raw HIT text and return an array of syntax/lint diagnostics.
    ///
    /// Each diagnostic has `severity`, `code`, `message`, `span`, `line` and
    /// `column` fields. The line and column are 1-based.
    #[wasm_bindgen]
    pub fn validate_hit(content: &str) -> JsValue {
        let diagnostics: Vec<Diagnostic> = validate(content);
        to_js_value(&diagnostics).unwrap_or(JsValue::NULL)
    }

    fn to_js_value<T: Serialize>(value: &T) -> Result<JsValue, JsError> {
        let json = serde_json::to_string(value)
            .map_err(|error| JsError::new(&format!("could not encode JSON: {error}")))?;
        js_sys::JSON::parse(&json)
            .map_err(|error| JsError::new(&format!("could not create JavaScript value: {error:?}")))
    }
}
