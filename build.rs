use std::env;
use std::fs;
use std::path::Path;

fn main() {
    let manifest_path = Path::new("tests/wasp/manifest.toml");
    println!("cargo:rerun-if-changed={}", manifest_path.display());

    let mut generated = String::new();
    let mut ids: Vec<String> = Vec::new();

    if let Ok(text) = fs::read_to_string(manifest_path) {
        if let Ok(value) = text.parse::<toml::Value>() {
            if let Some(cases) = value.get("case").and_then(|cases| cases.as_array()) {
                for case in cases {
                    let Some(id) = case.get("id").and_then(|id| id.as_str()) else {
                        continue;
                    };
                    let Some(rust_test) = case.get("rust_test").and_then(|name| name.as_str())
                    else {
                        continue;
                    };
                    ids.push(id.to_string());
                    generated.push_str(&format!(
                        "#[test]\nfn {rust_test}() {{ crate::support::run_case({id:?}); }}\n"
                    ));
                }
            }
        }
    }

    let ids = ids
        .iter()
        .map(|id| format!("{id:?}"))
        .collect::<Vec<_>>()
        .join(", ");
    generated.push_str(&format!(
        "\npub const GENERATED_CASE_IDS: &[&str] = &[{ids}];\n"
    ));

    let out_dir = env::var("OUT_DIR").expect("OUT_DIR is set by cargo");
    fs::write(Path::new(&out_dir).join("wasp_cases.rs"), generated)
        .expect("write generated WASP conformance cases");
}
