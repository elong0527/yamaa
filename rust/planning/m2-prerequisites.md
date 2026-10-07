# Shared compiler prerequisites for the fixed M2 cohort

For the current cross-issue delivery order after #1756 and the API/architecture
issues #1751–#1758, see [the migration order](migration-order.md). The baseline
audit below remains historical evidence, not a statement that model admission
or original-YAML host entrypoints still need to start from scratch.

Audited against PR [#1745](https://github.com/elong0527/yamaa/pull/1745), merged
as `875b18c205e9a0c4226b9f1040163db2d8ca384b` after all 11 final-head Actions
checks and full base-to-head review passed. This document describes remaining
work; the baseline qualifies assisted execution, not shared compilation.

## Current frontier

The installed wheel and independently source-rebuilt wheel each ran all 315
benchmarks on reference and assisted native routes. The six M2 cases already
match expected truth and portable observations on the assisted route. Their
remaining blocker is semantic ownership and the missing R original-YAML route,
not a need to shrink the documents or replace their evaluator truth.

## Inspected hosted baseline

[Native CI run 37531218070](https://github.com/elong0527/yamaa/actions/runs/37531218070)
completed Linux, macOS and Windows qualification of both the installed wheel and
source rebuild. The `native-qualification-ubuntu-24.04-3.14`,
`native-qualification-macos-15-3.14` and
`native-qualification-windows-latest-3.14` artifacts contain each form's
`coverage.json`, batch descriptors and full reports. All six inventories contain
945 rows and no gate errors; all six named cohort members pass at
`reference_assisted_run`. R original-YAML routes remain `not_exercised` with
`#1739:r_yaml_entrypoint_missing`. These are not shared compiler passes.

The tested checkout was synthetic merge
`35d12222a89875b732c653bac7f3c09638a23f4c`, whose verified parents are main
`7d9160451dc7e708629bac3803e764d421b51ee2` and PR head
`9d17a97343569e50925ec6dc50fc7079c9c3bd55`. Artifact identities include run,
attempt, platform and filename, with host 0.2.0 and native/core 0.1.0.

## Responsibilities that still belong to Python

| Boundary | Current owner | Required shared-path disposition |
| --- | --- | --- |
| YAML/schema closure admission and normalization | `native_specification.py`, shared schema/YAML services, host loader | Reuse owned shared schema snapshot and original YAML; filesystem authority stays in host ports |
| Inheritance traversal/composition, windows, source origins | shared schema/inheritance services with host orchestration | Compose through captured services without reconstructing a Python semantic model; retain entry/parent/source provenance |
| Normalized model shape and defaults | `specification/models.py`, `loader.py::load_specification_with_bundle` | Shared final document validator and typed representation; no `Specification.model_validate` acceptance gate |
| Expression single operation, intermediate unique checks, ordinal identifier, strict extras/types | Pydantic model declarations/validators | Explicit shared contracts or valid-but-unsupported scope; tests must exercise malformed and contradictory declarations |
| Submission and row-value metadata | `validate_submission_metadata`, `validate_value_metadata` | Outside bounded M2 execution vocabulary, explicitly reject unsupported declarations before study effects; migration-wide implementation remains required |
| Driver selection, operation admission, numeric/predicate/aggregate syntax, unsupported classification | `_native_dataset_plan.py::admit` and `planning/execution.py::preflight_execution` | Shared preflight over typed document; preserve semantic failure precedence and no source/callback work on Unsupported |
| Source declarations, CSV type inference/precision and resource capture | source adapters and resource ports | Hosts may retain IO/codec representation; shared contracts govern ordered declarations and binding. Source-schema binding remains after ingestion where required |
| Dependency schedule, lookup keys/selection, phase checks, row/default derivations | `planning/execution.py::plan_execution` (partly delegates to shared services) | Move orchestration and final decisions into shared compiler, reusing dependency/reference services |
| Dataset plan lowering and verification lowering | `_native_dataset_plan.py::lower`, `checks` | Shared immutable compiled specification; no generated Python normalized plan at R runtime |
| Native evaluation/checks | existing shared dataset engine | Reuse; retain handler/check ledgers and ordered failures |
| Accepted output/CSV serialization | host adapters | Share semantic output gates and compare exact CSV; retain host codecs only where contract is demonstrably identical |

## First compiler slice: acceptance rules to preserve

The current schema service already supplies `SchemaStructure::validate_document`,
`normalize_document`, `expand_named_windows`, `compose_layers`, and the engine's
`inheritance::traverse`. Their owned `Document` and captured schema snapshot are
the input to the next compiler slice; another host-shaped normalization format
is unnecessary. Retain existing budgets, ordered diagnostics and source identity.

`specification/loader.py` still runs `Specification.model_validate(strict=True)`
after normalization/window expansion. The following model contracts need explicit
shared coverage before that call is disconnected:

- Closed fields, strict scalar/container types and enum membership throughout the
  normalized document; booleans must not silently become integers.
- Exactly one operation in each expression, including nested intermediate key
  expressions and verification declarations.
- Nonempty intermediate uniqueness columns; an omitted check ID is permitted,
  but an explicitly null or empty ID is rejected.
- Source ordinal names match the existing ASCII identifier rule.
- Defaults and absence distinctions: source `empty_string=missing`, order
  direction `asc`, null ordering `last`, omitted handlers, and optional fields.
- Driver selection uses explicit `base`, otherwise the sole declared input;
  multiple inputs without a base do not select an arbitrary driver.

These are admission tests, not evidence that every feature executes. Preserve
valid-but-unsupported classification for declarations outside the bounded cohort,
and keep schema/model errors ahead of study effects at their existing boundary.
Submission and row-value metadata validation remains separately tracked; merely
dropping those fields from the typed representation would lose semantics.

Ownership tests must make calls to reference normalization,
`Specification.model_validate`, reference preflight/planning, Python dataset-plan
lowering and reference execution fail if reached by the shared route. Existing
assisted-route passes do not satisfy this test. R must obtain the same compiled
object from original YAML with no Python-produced document or plan.

## Fixed cohort prerequisites

- `adam-adlb-ordered-sum`: collected and grouped row templates, ordered SUM with
  missing handling, per-row overrides/defaults, unique and row_count checks.
- `schema-lookup`: primary DM driver; omitted and explicit lookup keys; fatal AE
  donor filtering, stable last selection, dependency on derived DTHCAUS; unique.
- `schema-window-functions`: complete named/inline window expansion, competition
  and dense ranks, ordered offsets, previous_non_missing, locf and baseline_flag;
  date precision, filtering and dependency phases.
- `schema-inheritance`: declared `spec_study.yaml`; both parent layers, declaration
  merge/source provenance, final document validation and inherited execution.
- `negative-zero-division` and `negative-integer-overflow`: checked numeric lowering,
  operand/failure order, exact portable diagnostics and no accepted output.
- Also replay inherited cycle/version/path failures from installed loader tests,
  explicit Unsupported before source/callback effects and host failure passthrough.

## Proposed bounded implementation sequence

1. Shared final document admission/typed representation and closed M2 vocabulary;
   replay independent model/schema diagnostics with no host semantic acceptance.
2. Shared source binding + simple source/literal/compute lowering; integrate an
   original-YAML R path immediately on the two failure fixtures.
3. Row templates/ordered reduction and its checks, then lookup binding/scheduling,
   then full window cohort and inherited entrypoints.
4. Qualify all six original documents from both installed hosts/package forms;
   disable Python semantic models/planning/lowering/evaluation in shared-path tests.
   Remove/disconnect only those bridge branches whose shared replacement qualifies.

No default cutover, POWER policy assumption, automatic golden regeneration or
full-language completion claim follows from this cohort. #1740/#1741/#1742 and
remaining #1585 release/cutover scope stay active.

The sequence above predates #1751–#1758. Its first two slices and the ordered-sum
part of the third have merged in #1747, #1750 and #1756. Portable diagnostics,
core-owned compiled representation and engine ports now precede the remaining
lookup/window/inheritance extensions; the linked migration order explains the
bounded public-facade gate and the later full-API gate.

## First implementation slice: normalized model admission

`schema/1::validate_model` now exposes core-owned final structural admission and an
immutable `SpecificationDocument`. It preserves the normalized tree and occurrence
indices, strict model types/closure, expression cardinality, intermediate unique
constraints, ordinal spelling, nested metadata shapes and diagnostic order. The
independent cases and installed reference mutations are component/compile evidence.

The next slice must consume effective defaults in a typed compiler representation,
carry the captured schema/source provenance through that entry point and connect
shared source binding/lowering. Submission/value metadata relationships still need
explicit implementation or Unsupported disposition before study effects. The
existing Python model call is not disconnected by structural service availability.
All six unchanged original-YAML cohort runs and the R entry point remain required.

## Next execution slice: original standalone failure runs

The bounded shared compiler now connects captured raw schema/YAML, structural
admission, source-independent preflight, lossless CSV ingestion, source binding,
source/compute lowering and checked dataset execution. Both owned host handles
exercise the original division-by-zero and integer-overflow fixtures; a shared
source-port observation path retains actual reads, cached snapshot counts and
source tables for complete failure-report comparison. See
[the compiler boundary](../SPECIFICATION_COMPILER.md) for its supported scope.

The report truth is independently authored and separately checked against the
reference implementation. This is a two-fixture integration slice, not completion
of #1739 or promotion of existing assisted inventory entries. Production source
capture/verification, stable complete schema/capture diagnostics, successful output
publication and the four remaining original cohort documents still require work.
R's wrapper retains original host errors/interruptions after native return; Python
keeps its original callback exception. No numerical policy or default cutover is
selected here.

## Ordered-sum compiler and publication slice

The shared entry point now lowers the unchanged ordered-sum document's record and
group templates, typed source fields, ordered SUM, unique and row_count checks.
An independent complete report pins all 17 rows and the original 1,030-byte CSV;
permanent installed Python/R tests exercise exact publication and original host
error/interrupt preservation. Verification declaration findings execute in the
shared engine after output keys and retain completed check records.

This adds a third original document to supplemental compiler integration tests.
It does not promote the fixed inventory or complete #1739. Remaining work includes
general row declaration/scope/default handling, lookup/window/inheritance documents,
complete host frontends and release platform evidence. The Python observer's
late-declaration ledger loss is fixed with independent complete-report regressions. The six-case cohort remains unchanged.


Row-default and reduction follow-up: source/literal defaults now inherit in each
template without an override; coverage and invalid aggregate-default conflicts
retain preflight ordering. Runtime numeric reductions skip missing values before
type checks and retain the authored operand in REQ-0510 diagnostics. Independent
reference and shared-run tests cover full reports and exact CSV for all-missing
text groups. Unknown grouping fields now receive a shared-style reference planning
diagnostic after ingestion instead of a Python grouping KeyError. Dependency-bearing
row expressions and broader phase relationships remain open.
