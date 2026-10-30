# HIT corpus

Place standard MOOSE `.i` input decks in this directory. The harness in
`tests/corpus.rs` discovers every `*.i` file automatically.

- Files whose names start with `invalid_` must produce at least one error
  diagnostic. They are used to verify error recovery.
- All other files must parse without diagnostics, format idempotently, and keep
  the same number of blocks, parameters, comments and array values after a
  parse → format → parse round trip.

Useful additions:

- decks from the MOOSE test suite,
- large real-world application inputs,
- unusual but valid syntax (multiline strings, nested arrays, dot paths),
- small focused malformed snippets named `invalid_*.i`.
