# Rule rewrite

This is the non-normative working draft for [issue #606](https://github.com/elong0527/yamaa/issues/606),
started from `main` at `67b0e56c`. The [current rules](../rules/README.md)
and schema comments remain authoritative until the replacement is complete.
This directory is not part of the normative rule index.

## Organization

Organize files by the question they answer. Give each behavior one owner;
other files link to that owner instead of repeating its requirements.
The paths below are relative to this directory. Only linked files have
been drafted; the other paths are the planned destination, not placeholders.

| Block | Files | Owns |
| --- | --- | --- |
| Specification | `specification/structure.md`, `composition.md`, `binding.md` | Declarations and column coverage; inheritance; names and scope |
| Values | [types](values/types.md), [numbers](values/numbers.md), [text](values/text.md), `values/temporal.md` | Value spaces, missingness, conversion, equality and order |
| Execution | `execution/lifecycle.md`, `rows.md`, `ordering.md`, `handlers.md`, `verification.md` | Evaluation phases; row construction; ordering terms; replacements; assertions and logs |
| Operations | `operations/expressions.md`, `predicates.md`, `computation.md`, `aggregation.md`, `lookup.md`, `windows.md`, `text.md`, `temporal.md`, `functions.md` | Operation inputs, evaluation, results, and failures |
| Storage | `storage/resources.md`, `ingestion.md`, `csv.md`, `parquet.md`, `publication.md` | Path confinement; source typing; format profiles; output selection and atomic publication |
| Submission | `submission/metadata.md`, `terminology.md`, `define-xml.md` | Governed metadata, codelists, and submission-document generation |
| Reference | `reference/schema-language.md` | Schema notation for maintainers |

The numbers draft covers representation and conversion. Arithmetic promotion,
overflow, and evaluation remain to be split from R010 during the computation
rewrite. The text draft covers values, not casing or mapping operations.

## Contract format and identity

Every draft has the same sections: Purpose, Scope and dependencies,
Requirements, Error conditions, Conformance examples, and Rationale.
Requirements and error conditions carry global IDs such as `REQ-0001`.
IDs have no file or topic prefix and survive file moves. Allocate the next
unused number; never renumber existing requirements or reuse retired IDs.
Rationale and examples explain contracts without adding requirements.

[migration.yaml](migration.yaml) maps legacy requirements to draft IDs.
Several old requirements may map to one rewritten requirement. A legacy
requirement split across new owners may map to several IDs. An unmapped
legacy requirement is pending, never implicitly retired. All legacy IDs
remain usable in the current rules and benchmark error contracts during
drafting. New IDs are draft identities, not yet accepted error citations.

The migration checker verifies global uniqueness, source and target existence,
coverage for source rules declared complete, and provenance for every draft
requirement. It reports remaining legacy coverage. It does not prove semantic
equivalence or runtime conformance.

## Sources of authority at cutover

The completed rewrite will give schemas ownership of shape, defaults, and
structural constraints, and contracts ownership of shared and operation-local
behavior. Schema descriptions will link to the owner. Generated field tables
will describe the schema without becoming another maintained contract.
Until cutover, the existing authority arrangement stays in force.

Closed syntax continues to come from [grammar/](../grammar/README.md).
Do not transcribe or alter a grammar while moving prose. Preserve current
vocabulary, including the `predicate` primitive and `assert` verification.
Do not revive retired `sql`, `mapping_from`, or handler `override` surfaces.

## Ownership migration plan

| Current owner | Destination | Progress |
| --- | --- | --- |
| R001 | execution/lifecycle, execution/rows | Pending |
| R002 | specification/binding | Pending |
| R003 | operations/lookup | Pending |
| R004 | operations/predicates | Pending |
| R005 | specification/structure, execution/lifecycle, storage/publication | Pending |
| R006 | reference/schema-language | Pending |
| R007 | operations/expressions, execution/ordering, operations/windows | Pending |
| R008 | execution/handlers | Pending |
| R009 | execution/verification | Pending |
| R010 | values/numbers, operations/computation | Representation drafted; arithmetic pending |
| R011 | values/types, values/numbers | All numbered requirements mapped to drafts |
| R012 | operations/text | Pending |
| R013 | operations/aggregation | Pending |
| R014 | storage/ingestion | Pending |
| R016 | values/temporal, operations/temporal | Pending |
| R017 | specification/composition | Pending |
| R018 | operations/functions | Pending |
| R019 | specification/structure, values/text, operations/text | Text values drafted; source notation and operations pending |
| R020 | storage/csv, storage/parquet, storage/publication | Pending |
| R021 | storage/resources | Pending |
| R022 | operations/text | Pending |
| R023 | storage/ingestion, storage/csv | Pending |
| R024 | submission/metadata | Pending |
| R025 | submission/terminology | Pending |
| R026 | submission/define-xml | Pending |
| R027 | storage/parquet | Pending |
| Schema-local behavior | Owning operation or execution contract | Inventory and migration pending |

R015 is already retired. Keep input and output differences explicit within
each storage profile; sharing a file does not imply identical contracts.
The single lifecycle will own the phase sequence, while operation files
refer to their evaluation phase without restating the whole sequence.

## Completion gates

1. Rewrite the remaining blocks and inventory schema-local behavior.
2. Map every old requirement and schema-local behavior to a replacement or
   an explicitly reviewed retirement. Resolve source contradictions openly;
   an editorial rewrite must not choose new behavior silently.
3. Review each requirement against positive and negative fixtures. Add
   conformance coverage where the current examples leave a gap. Preserve
   exact fixture bytes and expected outcomes unless a behavior change is
   separately identified and approved.
4. Update validator citations, generated documentation, and R/Python
   conformance references together. Enable legacy-ID resolution through
   the migration map before removing old requirement definitions.
5. Move the completed contracts into `yaml/rules/`, update its index and
   agent guidance, and retire superseded files in one reviewed cutover.

## Questions retained for review

- R010-3 still names the retired R015 lookup contract. Resolve that stale
  reference against current R003 while rewriting computation and binding;
  do not restore the retired lookup surface.
- R011-6 describes ISO 8601 text as ordering chronologically without stating
  which text forms that claim covers. REQ-0003 preserves the existing claim
  for traceability. Review it against R016's closed forms and R019's scalar
  order before admitting the replacement; do not broaden temporal parsing.
- R011-24 says widening chooses the nearest binary64 value without naming
  tie behavior. The draft preserves that wording. Check the existing runtime
  and conformance contracts before making any stronger rounding claim.

## Validation

Run from the repository root:

```sh
python .github/scripts/yaml-validation/check_rule_rewrite.py
```

The existing repository, documentation, grammar, and conformance checks
continue to validate the normative specification. Passing draft checks
alone does not admit these files as normative rules.
