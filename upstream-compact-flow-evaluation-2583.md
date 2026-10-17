# Upstream compact-flow parser evaluation (yamaa #2583)

Status: blocked. No published `saphyr-parser` release contains the
compact-flow fix, so the vendored patch stays. This note records the
evaluation; it changes no build input.

## Verdict

- Do NOT swap. The newest published `saphyr-parser` (0.2.2) fails four of
  the five independent compact-flow controls, and upstream has not accepted
  the fix. The `[patch.crates-io]` entry and `rust/vendor/saphyr-parser`
  remain in place.

## Versions tested

| Version | Source | Compact-flow fix present |
| --- | --- | --- |
| 0.2.1 | crates.io release backing `rust/vendor/saphyr-parser` (see its `PROVENANCE.md`) | No; the vendored `compact-flow-pairs.patch` adds it |
| 0.2.2 | crates.io, published 2026-10-15 by `davvid`, checksum `6c6220...25d387` | No |

crates.io reports `max_version = newest_version = 0.2.2`; nothing newer
than 0.2.2 exists. The 0.2.2 changelog lists one change only: raw NUL
characters are now rejected (upstream pull request #135). Its
`src/scanner.rs` still carries the old scalar `flow_mapping_started: bool`
logic that the vendored patch replaces with per-flow-level state.

## Upstream acceptance check

- `parser/src/scanner.rs` history on `saphyr-rs/saphyr` main: latest touch
  is the 2026-10-09 NUL fix. No commit addresses compact-flow pairs.
- Open pull requests (9) and issue search for compact flow: no proposal
  covers nested-mapping commas closing an outer compact pair.
- Therefore: no upstream acceptance and no published fixed version to
  record. Nothing here claims publication on behalf of upstream.

## Control results against unmodified 0.2.2

Method: scratch crates depending on registry `saphyr-parser = "=0.2.2"`
(cargo checksum-verified, never vendored sources) ran the five tests in
`rust/vendor/saphyr-parser/tests/compact_flow_pairs.rs` verbatim.
Result: 1 passed, 4 failed.

| Control | Unmodified 0.2.2 | Vendored patched crate |
| --- | --- | --- |
| `compact_pair_keeps_every_entry_of_its_nested_mapping` | FAIL | pass |
| `preceding_mapping_does_not_change_compact_pair_events` | FAIL | pass |
| `compact_pairs_survive_nested_sequences_and_mapping_values` | FAIL | pass |
| `each_sequence_entry_has_its_own_pair_state` | FAIL | pass |
| `mismatched_or_missing_closers_return_the_same_error_without_panicking` | pass | pass |

Failure shape on 0.2.2 (example `[k: {a: 1, b: 2}]`): the comma inside the
nested mapping closes the outer compact pair, splitting `{a: 1, b: 2}`
into two mappings with a spurious empty scalar, exactly the defect the
vendored patch fixes.

## YAML conformance corpus against unmodified 0.2.2

Method: the published 0.2.2 `tests/yaml-test-suite.rs` harness copied
verbatim into a scratch crate, run against registry `saphyr-parser = "=0.2.2"`
plus registry `saphyr = "=0.2.2"`, with the current `yaml/yaml-test-suite`
data files (351 files). Parser sources untouched.
Result: 402 passed, 0 failed.

The corpus passes because it does not cover the compact-flow shape; the
five independent controls remain the binding gate. The crate's own unit
tests (`cargo test --lib` on the published 0.2.2 sources) also pass
(5 passed).

## In-repo baseline (unchanged tree)

- `cargo test --locked -p saphyr-parser --test compact_flow_pairs` from
  `rust/`: 5 passed (vendored patched crate).
- `cargo test --locked -p yamaa-adapters yaml_decode` from `rust/`:
  `yaml_decode::reader::tests::compact_flow_pairs_keep_nested_mapping_entries`
  passes (20 passed, 0 failed in the `yaml_decode` filter).
- No file under `benchmarks/**/input` or `benchmarks/**/expected` was
  touched or needed.

## What would unblock the swap

1. Upstream merges a compact-flow fix and publishes a release containing
   it (a version newer than 0.2.2, or a re-release with the fix).
2. That unmodified published crate passes all five controls above plus
   the YAML conformance corpus.
3. Only then: remove the path patch and vendored implementation, update
   `Cargo.lock` and licenses/provenance, and run the source-packaging,
   core/adapter, and installed Python/R gates.
