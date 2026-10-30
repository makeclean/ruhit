//! 1:1 WASP conformance tests.
//!
//! `build.rs` reads `tests/wasp/manifest.toml` and generates one `#[test]`
//! per WASP test.  The meta-test below enforces that the generated set and the
//! manifest stay in lockstep.

#[path = "wasp/support/mod.rs"]
mod support;

include!(concat!(env!("OUT_DIR"), "/wasp_cases.rs"));

#[test]
fn manifest_is_one_to_one() {
    support::assert_manifest(GENERATED_CASE_IDS);
}
