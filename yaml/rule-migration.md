# Rule contract cutover

This non-normative record explains the replacement of the numbered rule files
and the temporary `rules-next` drafts. The baseline is main at `67b0e56c`.
The active [index](rules/README.md) now contains 30 contracts in seven blocks.

## Scope and preservation

All 1,071 numbered baseline requirements resolve through
[migration.yaml](rules/migration.yaml). The 30 IDs allocated in the first draft
remain stable. Existing forwarding paragraphs and repeated conversion tables
are consolidated through aliases; the normative text has no legacy forwarding
files. Rule filenames and requirement identities are independent.

The migration also inventories all 332 baseline schema descriptions and comment
blocks, including module navigation comments. Their text is retained as audit
evidence in the migration map. Interface meanings now live with the owning
contract, and each schema description links to that requirement. Generated
field tables show schema structure and defaults without restating semantics.
Parsed schema structure, constraints, registries, defaults, and includes are
unchanged after removing descriptions for comparison.

Negative fixtures, runtime diagnostic citations, and the governed warning log
now use canonical requirement IDs. Only the warning log's `REQUIREMENT` column
changes; its values, keys, messages, and severity are preserved. Dataset golden
values are unchanged. Historical diagnostic IDs remain accepted and resolve
through the migration map. Validation-condition families retain their legacy
names for compatibility; they do not identify a second set of rule files.

Grammar ownership now points to the corresponding operation contract. Grammar
productions, vocabularies, parser cases, and conformance expectations remain
unchanged. The generated grammar blocks retain the exact source productions.

## Editorial conflicts resolved

- The old row-construction requirement R001-8 said a grouped candidate
  completed stages 1 through 4 before filtering, while R005-25 and R005-27
  explicitly put whole-column verification after candidate filtering. The
  rewritten rows contract refers to stages 1 through 3, preserving the existing
  lifecycle and runtime behavior: discarded candidates are not verified.
- References to retired R015 now point to the current lookup contract.
  Lookup dependencies use `key_base` and `between.value`, and paired-list
  validation uses `key_base` and `key`. No retired lookup surface is restored.
- The lookup introduction's example incorrectly used `key_base` as a
  derivation keyword. It now uses `source`, matching the schema and runner.
- The broad statement that ISO 8601 text orders chronologically is narrowed
  to the canonical fixed-width text of values of one temporal type. Arbitrary
  strings continue to use scalar order. Accepted temporal text is unchanged.
- The temporal conversion matrix delegates to the single type-conversion
  matrix. General comparison and aggregate-context forwarding requirements
  resolve directly through aliases to their existing owners.

These changes reconcile prose with existing contracts; they do not change
evaluation or admit new inputs. Undefined integer-widening tie behavior is
not expanded by this editorial cutover: the existing nearest-binary64 wording
is retained. A stronger policy requires a separate semantic decision.

The upstream readability edits in #614 and #617 are carried into their new
contract owners. The fixed baseline inventory remains unchanged so that its
provenance continues to describe the original source snapshot.

## Validation and limits

The migration check rejects missing source entries, duplicate definitions,
unresolved targets, wrong owners, missing reverse provenance, unindexed
contracts, broken file links, and schema descriptions without a canonical
owner. Regression tests exercise ID movement and failures that could otherwise
silently lose coverage. Generated references are checked for drift in CI.

The repository's Python execution suite and shared R grammar vectors provide
runtime evidence within their existing scope. This migration does not claim
that every proposed language feature has an R implementation. Existing blocked
benchmarks and the documented regex-consumer limitation remain explicit.

## Maintenance

Edit the owning Markdown contract for behavior; edit the schema for structural
changes. Keep canonical IDs stable and update provenance when ownership moves.
Regenerate reference tables with:

```sh
python .github/scripts/yaml-validation/generate_rule_reference.py
```

The migration inventory is historical evidence, not a source from which future
contract text should be regenerated. Review new semantic decisions on their
own merits and add conformance examples for them.
