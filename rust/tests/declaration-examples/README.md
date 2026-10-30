# Declaration examples

Six benchmarks converted by hand to the 1.0 surface during the 1.0 interface
design (#3030 Phase 0 item 4), applying the
[rename table](../../../tools/migrate/renames.yaml) exactly and writing out the
join keys that were then inferred. The design record that introduced them,
`docs/design/interface-1.0/examples.md`, is kept at tag `yamaa-v0.6.2`.

They are test fixtures, not runnable benchmarks: they carry no inputs and keep
each benchmark's relative paths (`input/...`, `define.xml`). Their readers are:

- `rust/crates/yamaa-adapters/tests/declaration_v1_schema.rs`, which decodes
  every file against the shipped 1.0 schema;
- `rust/crates/yamaa-application/tests/declaration_project_files.rs` and the
  `project_files` cases under that crate's `tests/fixtures/diagnostics/`;
- `rust/crates/yamaa-cli/tests/cli.rs`, whose submission study starts from
  `sdtm-dm-metadata`;
- `tools/tests/test_schema_v1.py`, which validates every file against the
  generated JSON Schema.

`schema-inheritance` has no child-layer null, so its conversion writes no
`clear:`; its sibling `schema-inheritance-clear` shows `clear:`.
