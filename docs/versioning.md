# Versioning and conformance

This policy governs changes after the fresh 0.4.0 release. Python, R and the
shared Rust engine use one coordinated `MAJOR.MINOR.PATCH` release number.
The approved 0.4.0 removals in the [migration guide](release.md#approved-decisions)
remain in force: remove obsolete interfaces and callers together, without
compatibility aliases or a historical execution mode.

## What a version promises

The public contract includes the documented Python and R APIs, admitted
specifications and schema defaults, language semantics, source profiles,
logical values and ordering, promised artifact bytes, diagnostics and their
structured fields, and conformance assertions. Private Rust interfaces and
repository tools have no independent public API guarantee. A change to them
still follows this policy whenever it affects a public result.

Classify a release by its most disruptive change:

| Change | From 1.0.0 onward | After 0.4.0 and before 1.0.0 |
| --- | --- | --- |
| Remove or rename a public API/field; narrow accepted inputs; change a default, previously valid result, promised bytes, diagnostic condition/requirement/path/context, or required host/dependency support | Major | Minor |
| Add an API, expression, optional field, source profile or diagnostic for a newly admitted feature, while preserving every existing contract | Minor | Minor |
| Fix implementation behavior to match the already published contract and independently authored truth; improve performance without observable changes; correct prose; add coverage of existing requirements | Patch | Patch |

Thus `0.4.1` preserves the 0.4 contract; `0.5.0` may introduce a documented
break. Pre-1.0 does not permit an arbitrary break in a patch. The move to
`1.0.0` declares a stable baseline and may contain documented migration changes.
Prereleases such as `0.5.0-rc.1` are previews of that upcoming baseline; they
are not the compatibility promise of a final release.

A bug fix may change an incorrect observed result in a patch only when the
published requirement and independent truth already decide the result. Name
that requirement and affected results in release notes. Ambiguous or newly
chosen behavior needs a minor addition or a breaking boundary, as above.
Changing the expectation to agree with the engine does not establish a bug fix.

Schema bundle tokens, currently `"1.0"`, use the existing `MAJOR.MINOR`
spelling and exact equality admission. They are separate from package SemVer.
A compatible optional field addition may retain the token and needs a package
minor increment. When declaring a new bundle token, use a minor token for an
additive shape, or a major token for a field removal/rename, changed default or
narrower structural constraint. Editorial corrections keep the token. Language
changes that leave schema shape and defaults intact can also keep it, while
still requiring the package increment in the table.

A new bundle token is an explicit migration: older documents are not silently
reinterpreted or upgraded. Under the current single-bundle admission contract,
even an additive `1.0` to `1.1` token change rejects existing `1.0` documents and
therefore requires a breaking package boundary. Admitting both tokens would
require a separately implemented and tested contract.

## Deprecations and previews

After 0.4.0, announce a planned removal in release notes and the owning API or
language documentation, giving the first deprecated release, replacement,
migration example and earliest removal release/date. Keep the existing
implementation through its notice window; do not add obsolete spellings or
aliases to extend it. Removal is allowed only at a breaking boundary and after
both minimums below:

| First deprecated in | Minimum notice before removal |
| --- | --- |
| A final 0.x release after 0.4.0 | One subsequent final minor release and 90 days |
| A final release at or after 1.0.0 | Two subsequent final minor releases and 180 days |

For example, a deprecation announced in `0.5.0` remains available in `0.6.0`
and can be removed in `0.7.0` after 90 days; one announced in `1.2.0` remains
through `1.3.0` and `1.4.0` and can be removed in `2.0.0` after 180 days.
The same window applies across the 1.0 boundary. These windows do not
retroactively deprecate or restore anything removed by fresh 0.4.0.

A notice in documentation is sufficient; a runtime deprecation diagnostic is
an additional public behavior change. Add one only with an explicit contract,
registered condition, authored expectation and compatible host exposure. Do
not inject new findings into existing results in a patch merely to advertise
a future removal. An urgent correctness or security refusal still follows the
release classification above; a shorter removal window requires an explicit
owner decision recorded in the release notes.

0.4.0 has no language-version switch or preview-behavior flags. A future
preview may be introduced only in a minor release, with an explicit opt-in,
unchanged behavior when absent, a named contract, diagnostics and independent
tests for both selections. It is available for at least one subsequent final
minor release and 90 days before promotion or removal. Its introduction notes
must state the earliest decision release/date; any extension updates those
notes. A preview is excluded from the default compatibility promise, but its
selection and changes must be documented in every affected release. Promotion
that changes existing default behavior requires a breaking boundary. Remove
the expired flag and callers together; no permanent legacy-mode shim is promised.

## Selecting and citing a contract

Install an exact package release and retain the source revision and schema
token with the specification. `schema_version: "1.0"` selects the installed
bundle; it does not pin all semantics across package releases. A contract
citation therefore consists of the package release, exact source revision,
schema token and permanent requirement ID. Keep host/runtime and dependency
versions with a reproducibility record as described in the
[introduction](articles/intro.md#programs-in-a-study-submission).

Diagnostic rows carry the registered `condition`, `requirement`, `spec_paths`
and structured facts in `context`, exposed as JSON text by the host APIs.
Decode that JSON to inspect those facts. Interpret requirement IDs against the
exact revision's rules and condition registry, not mutable `main`. The current row
format does not repeat a package version in every finding: retain its installed
package/core version and specification alongside the report. A schema refusal
identifies the selected bundle in `schema_version_mismatch` context `expected`
and the authored token in `actual` (or its `actual_type` when it is a container).
Inherited layers must agree with the same bundle; mismatch precedes later
field checks. Registry-owned null-requirement exceptions retain their existing
meaning. A future preview's admission contract must additionally record its
selected name/version in structured diagnostic context, including failures,
so results from distinct selections cannot be confused.

## Versioned conformance suite

The suite has the coordinated release version: `yamaa-conformance/0.4.0`
denotes the complete authored contracts, fixtures, comparators and consumers
at the `yamaa-v0.4.0` release tag. Its immutable reference is that tag's exact
commit. A working branch is identified as `yamaa-conformance/<version>-dev`
plus its full commit; the package number alone never establishes qualification.
Publishing the tag remains subject to the [release gates](release.md#publication).

Suite scope includes the benchmark inputs and independent expectations, YAML
grammar and regex vectors, shared Rust fixtures and tests, and installed Python
and R qualification consumers. See the
[fixture guide](https://github.com/elong0527/yamaa/blob/main/yaml/conformance/README.md)
and [installed checks](https://github.com/elong0527/yamaa/blob/main/rust/qualification/README.md).
`public-conformance/1` is the runner wire protocol, not the suite version;
fixture-specific versions such as regex `contract_version: "2.0.0"` likewise
identify that fixture contract, not the whole language.

Use a commit-pinned repository URL and a relative case identity for a stable
citation: `benchmarks/<name>/<entry>` for benchmarks, fixture path plus case
`id` for vectors, and test file plus test name for executable controls. Never
reuse an ID for different semantics. Retirement and renames remain traceable
in Git history and release notes; current discovery contains current cases.
For example, [the regex vectors at this development revision](https://github.com/elong0527/yamaa/blob/7f9fe979b073ebf070e075e1d4276dda617ed390/yaml/conformance/regex.yaml)
plus case `ascii-digit-class-rejects-arabic-indic-digit` identify one fixed
expectation, independently of later fixture additions.
To run an older suite, check out its exact revision and use its own comparators
and consumers. Do not combine old truth with the latest comparator. Native
reports already retain `source_revision` and installed package/core versions;
these bind the observed results to the selected suite and engine.

Every suite update follows this process:

1. Identify the owning requirement and classify the intended public change
   using the table above. Apply that increment to the coordinated package and
   suite release; multiple changes can share an unreleased version. Coverage
   additions that decide no new behavior are patch changes.
2. Author the expected outcome independently before changing the engine. Add
   positive, negative and boundary controls and equivalent host coverage where
   applicable. Preserve existing benchmark input and expected files; a deliberate
   truth correction needs independent reproduction and an explicit PR rationale.
3. Update the owning rules/schema/grammar first where semantics change, keeping
   permanent REQ IDs. Change a fixture-specific contract version only when its
   own contract changes. Record migrations, deprecations or preview selections
   in the release notes; do not commit passing-status ledgers or generated truth.
4. Run the affected checks in
   [CONTRIBUTING.md](https://github.com/elong0527/yamaa/blob/main/CONTRIBUTING.md#develop-and-verify).
   Qualification runs the entire current active suite on installed hosts; a
   new active benchmark is required automatically. Retain run evidence in the
   PR/release, with exact source revisions and artifact references.
5. Publish the coordinated release tag once its final qualification passes.
   Never move a published tag or rewrite its expectations. A correction belongs
   in a new version with new evidence.

## Executable compatibility examples

These existing controls make the policy's observable boundary concrete. They
are deliberately independent of the engine; the examples classify a proposed
change, not a claim that a future release or deprecation has shipped.

| Proposed change | Classification | Existing executable evidence |
| --- | --- | --- |
| Add a benchmark for an existing requirement, leaving prior truth intact | Patch | `test_new_directory_is_required_on_both_hosts_without_a_registry_edit` requires the new case in both host inventories. |
| Change only Parquet metadata or physical serialization while preserving all promised logical fields and values | Patch | `test_parquet_logical_comparison_ignores_metadata_and_serialization` checks the preserved logical contract. |
| Change a CSV cell, header, row order/count, line ending, or quoted-empty/missing distinction | Breaking unless correcting a proved implementation defect | `test_retired_adapter_csv_mutations_cannot_pass_public_conformance` and `test_quoted_empty_is_distinct_from_missing_and_csv_bytes_are_exact` reject these differences. |
| Change a negative case's condition, authored path or required context fact | Breaking unless correcting a proved implementation defect | `test_negative_uses_first_issue_and_context_subset_without_reference` and `test_validation_truth_requires_every_path_and_allows_independent_findings` pin those facts. |
| Add an optional operation that existing specifications never select | Minor, provided all existing results remain unchanged | The current benchmark comparator gates every active case; the new operation also needs independently authored positive and negative cases. |

The named comparator controls live in
[test_public_conformance.py](https://github.com/elong0527/yamaa/blob/main/tools/tests/test_public_conformance.py).
The shared Rust test `version_precedes_field_findings_and_classes_keep_schema_then_input_order`
in [schema_references.rs](https://github.com/elong0527/yamaa/blob/main/rust/crates/yamaa-core/tests/schema_references.rs)
additionally proves explicit schema refusal before ordinary field findings.
