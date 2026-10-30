//! Resilience tests: malformed input must never panic, and every AST must be
//! formattable without panicking.

use std::panic::{catch_unwind, AssertUnwindSafe};

use ruhit::{parse, serialize, validate};

const PATHOLOGICAL: &[&str] = &[
    "",
    " ",
    "\n",
    "\r\n",
    "[",
    "]",
    "[]",
    "[../]",
    "[.]",
    "[[]]",
    "[[[[",
    "]]]]",
    "a=",
    "a =",
    "a = '",
    "a = \"",
    "a = '\\",
    "a = [",
    "a = [1,",
    "a = [1 2 3",
    "a = [1, [2, [3",
    "a = [[], []]",
    "a = [\n []\n]\n",
    "[Block",
    "[Block]\n[]",
    "[Block]\n  []",
    "[Block]\n  broken\n[]",
    "[Block]\n  = 1\n[]",
    "[Block]\n  a == 1\n[]",
    "[Block]\n  a = ,\n[]",
    "[Block]\n  a = 1,,2\n[]",
    "[Block]\n  a = 1 # comment\n[]",
    "[Block]\n  a = 1, # comment\n[]",
    "[Block]\n  a = [1, # comment\n[]",
    "[]\n[]\n",
    "[./x]\n[../]\n[../]\n",
    "é = '✓'\n",
    "a = 1e999\n",
    "a = \0\n",
    "!include file.i\n",
];

#[test]
fn pathological_inputs_do_not_panic() {
    for source in PATHOLOGICAL {
        assert_parses_and_formats(source);
    }
}

#[test]
fn pseudo_random_inputs_do_not_panic() {
    const ALPHABET: &[u8] =
        b"[]=,#'\"\\ \n\t\r./abcXYZ0123456789eE+-_!(){}:;";
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);

    for _ in 0..750 {
        let length = (rng.next() % 180) as usize;
        let mut source = String::with_capacity(length);
        for _ in 0..length {
            let index = (rng.next() as usize) % ALPHABET.len();
            source.push(ALPHABET[index] as char);
        }
        assert_parses_and_formats(&source);
    }
}

#[test]
fn deeply_nested_blocks_are_bounded() {
    let depth = 700;
    let mut source = String::new();
    for _ in 0..depth {
        source.push_str("[B]\n");
    }
    for _ in 0..depth {
        source.push_str("[]\n");
    }
    let document = parse(&source);
    assert!(document
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == "nesting-too-deep"));
    assert_parses_and_formats(&source);
}

#[test]
fn deeply_nested_arrays_are_bounded() {
    let depth = 700;
    let mut source = String::from("[Block]\n  values = ");
    for _ in 0..depth {
        source.push('[');
    }
    source.push('1');
    for _ in 0..depth {
        source.push(']');
    }
    source.push_str("\n[]\n");

    let document = parse(&source);
    assert!(document
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == "nesting-too-deep"));
    assert_parses_and_formats(&source);
}

fn assert_parses_and_formats(source: &str) {
    let document = catch_unwind(AssertUnwindSafe(|| parse(source)))
        .unwrap_or_else(|_| panic!("parse panicked for input: {source:?}"));
    let _ = catch_unwind(AssertUnwindSafe(|| validate(source)))
        .unwrap_or_else(|_| panic!("validate panicked for input: {source:?}"));
    let formatted = catch_unwind(AssertUnwindSafe(|| serialize(&document)))
        .unwrap_or_else(|_| panic!("serialize panicked for input: {source:?}"));
    let _ = catch_unwind(AssertUnwindSafe(|| parse(&formatted)))
        .unwrap_or_else(|_| panic!("parse panicked for formatted input: {source:?}"));
}

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        // Numerical Recipes LCG; deterministic and dependency-free.
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0
    }
}
