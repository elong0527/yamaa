# Completed-row selected assertions

This implements the remaining selected-field site in #2005. A specification that
constructs `VALUE: P.X` can already read the declared kept record. Before this
change, changing an otherwise valid assertion from `VALUE = 2` to `P.X = 2`
produces `unknown_field` / REQ-0405. REQ-0183 and REQ-0185 permit that declared
lookup for the completed row.

The compiler holds declared assertion lookup names until
the existing source-schema binding step. It binds their names to completed
columns or admitted selected fields. Unknown ordinary names keep their original
finding; a missing declared lookup field produces the same cause once its schema
is available. A valid `when` retains its type checkpoint before a later invalid
`require`, and earlier verification records retain their order and failures.

Verification borrows the original completed candidates, including their template,
feeding members and selection identity, and the attempt's existing intermediate
Run. A kept record or derived relation is selected/materialized by the same native
context already used for case operands. The owned output table is formed after
verification. No table or host error is cloned, and no new evaluator or endpoint
is introduced.

Type admission uses compiled column and selected-field types with nonmissing
representatives. It has no study-cell, selector or function-call authority. Actual
rows evaluate `when` followed by `require`, including exempt rows, as REQ-0383
requires. Existing AND/OR operand evaluation remains eager. Existing function
missing-input short circuiting is unchanged. Predicate selected reads preserve
the existing policy that their inherited recovery does not add handler counts.

The shared operand context requires an assignment-shaped path carrier. Assertion
admission excludes raw source, collection and joined bindings; the carrier's
column/expression do not participate in selected resolution. Original opaque
selected-cell failures retain `intermediates[0]` and their original source
coordinates/payload. No-match failures retain their completed output keys. Neither
invents a destination column identity.

The independent fixture `completed_lookup_assertions.json` contains 21 complete
Check/Build findings and ordered verification records, typed output cells,
capture/handler ledgers and exact output CSV bytes. It covers both predicate
sites, shared selected fields, record/group/mixed templates, absent selection,
eager WHEN/REQUIRE and logical operands, missing fields, ordered declaration/type
failures, zero-output admission and warning severity with its required warning
log. Four additional cases retain the original missing-field finding in warning
WHEN/REQUIRE assertions, with and without row templates, while a later warning
check remains declared. Both installed hosts consume the same authored truth
through portable ordinal directories. No existing fixture or benchmark
expectation is changed.

Six independently authored typed engine controls additionally pin actual cell
traces, fresh attempts, non-cloneable payload identity and failure order, zero
donor cells on empty-output admission, and derived donor callback counts. One
materialization executes two callbacks per attempt; subsequent WHEN/REQUIRE reads
execute none. Empty output executes none.

The older #1980 wording `source.condition` is outside the current source grammar.
`condition` belongs to `flag_class`; it remains an unknown source field. The
grouped-filter historical private fixture disposition and every earlier source
and numerical qualification head remain unchanged by this assertion work.

## Original #2005 obligations

| Original feature | Independent controls and required observations |
| --- | --- |
| `case_source_predicate` | `yamaa-adapters/tests/declarative_case_predicates.rs` exercises implicit feeding-value reduction, multiple-value failure, three-valued branch choice, record defaults, explicit record assignments and grouped assignments. The same case expression contains a qualified source predicate and full-pattern extraction in each position. `yamaa-engine/tests/case_predicates.rs` retains typed scope and effect controls. |
| `case_source_filter` | `case_predicates.json`, `source_filter_scope.json` and `source_filter_mixed.json` retain ordinary, record, grouped and mixed-template cases. The scope and mixed fixtures contain 12 and three complete authored cases respectively; `declarative_source_selection.rs` and `declarative_case_predicates.rs` execute their scope/failure controls. Original `sdtm-ds-reasons` CSV truth is compared without modification. |
| `assert_predicate_text` | `original_assertions.tsv` contains 17 complete independently authored assertion reports, including successful, failed, inactive, missing, empty-output and malformed declarations. `specification_run.rs` compares complete reports and successful CSV bytes. `declarative_case_predicates.rs` additionally checks the same predicates after column-case, record and grouped construction. |
| `assert_predicate_diagnostic` | The original assertion reports retain declaration/type failure order and the completed verification prefix. `yamaa-adapters/tests/predicate_diagnostics.rs` and `yamaa-engine/tests/predicate_diagnostics.rs` retain registered predicate causes. Both `when` and `require` remain eager, including exempt rows. |
| `verification_literal_diagnostic` | `original_column_values.tsv` contains 20 complete authored reports; `row_column_checks.tsv` contains 48. Their `specification_run.rs` replays retain allowed values, missing literals, kind mismatches, declaration order, converted values, keys and prior verification records. The case-position controls retain non-text assertion admission findings. |
| `verification_numeric_bound` | The same column/row report suites and the case-position controls exercise numeric range bounds after completed construction. Finite exponent-form bounds are admitted. Full-range signed-i64 behavior and the invalid-carrier disposition remain separate explicit contracts. |
| `str_extract_pattern` | `text_extract_reference.json` preserves 24 independent extraction witnesses. `declarative_text_extract.rs` exercises original positive/negative artifacts, captures, missing/no-match handlers, empty captures, nested cases, row extraction and failure order. `declarative_case_predicates.rs` executes lookbehind/lookahead capture in column cases, record defaults, explicit records and groups. |

All fixture paths in this table are relative to
`rust/crates/yamaa-adapters/tests/fixtures/`, and Rust test paths are relative to
`rust/crates/`, unless otherwise specified. Earlier slice descriptions remain
historical: their former unsupported lists do not describe the merged engine.

## Installed hosts and unchanged truth

The cataloged `python/original-specifications` and `r/original-specifications`
suites execute the same independent fixture families through installed public
Check/Build/Save. Python forbids the reference semantic imports; R executes
without Python in its PATH. The suites compare complete ordered findings and
records, typed output cells, retained effects and exact Save bytes. Portable
ordinal staging preserves all 21 selected-assertion cases in both package forms.

The [scope audit posted on #2005](https://github.com/elong0527/yamaa/issues/2005#issuecomment-6153143401)
retains the complete original benchmark lists: 77 verification sites, 88
assertion sites, 20 case-source predicate sites, five extraction sites and one
case-source filter site (122 distinct benchmarks). These structural supersets are scope evidence, not
claims that every member previously triggered a rejection. Every listed
benchmark is present in both host lists of
`rust/qualification/public-conformance.json`. The ratchet requires all 316
active benchmarks to pass on every installed lane. Its two historical path
exclusions are outside this issue's scope. No benchmark expectation or original
independent fixture has been regenerated for acceptance.

Three dispositions remain explicit:

- Raw integer carriers outside signed-i64 are invalid under REQ-0014; finite
  exponent-form numerical bounds remain supported.
- `condition` belongs to `flag_class`. The obsolete `source.condition` spelling
  retains the ordinary unknown-field finding under the current source grammar.
- `grouped_key_filter` preserves its frozen historical private-reference CSV in
  `case_predicates.json`. `case_predicate_dispositions.json` identifies the
  reference defect and points to the separate independently authored normative
  REQ-0148 rejection in `source_filter_scope.json`. A grouped scalar key cannot
  be filtered; its historical output is not a current passing claim.

## Qualification required for closure

The issue can be closed when its merged implementation has a successful full
combined-main native run with the retained independent audit, including all 18
Python interpreter/platform/package-form lanes, all three R hosts, the active
public ratchet, installed supplemental suites, shared Rust checks and deferred
numerical qualification. Full branch qualification additionally qualifies any
follow-up before merge, including Windows R. Its source revision, run attempt
and artifacts must be retained in the PR; it cannot substitute for the
combined-main run.

The shared release evidence owner is #1996. No approval, passing matrix or
completed acceptance checkbox is inferred from this document. Retain the final
run and audit links in the implementing PR before closing #2005.
