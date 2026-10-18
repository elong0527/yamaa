# Verification

Verifications assert properties of completed values: column verifications over
one converted column, dataset verifications over the completed output. A failed
`error` verification fails the run; a violated `warning` fails nothing. Every
run builds one report recording its inputs, row templates, handlers, columns
and every check it evaluated, held or violated.

## Running verifications

<a id="check-1"></a>

### CHECK-1. Column checks run after their column converts; dataset checks run after all column work

`schema_verification.yaml` registers column checks in `column_verifications`
and completed-dataset checks in `dataset_verifications`. A column verification
infers its declared column and runs after that column's derivation and
conversion. Dataset verifications run after all column work, output-key
validation, and column verifications finish.

Evidence: `schema-verification-log`, `negative-single-verification`, `rust/crates/yamaa-adapters/tests/fixtures/original_not_missing.json`.
Replaces: REQ-0371, REQ-0372.

A column's `verifications` and the dataset's `verifications` are each a list
of checks, and a single check is written as a one-item list. A check written
on its own, not in a list, fails as `invalid_field_type` under
[SCHEMA-12](reference/schema-language.md#schema-12).

<a id="check-2"></a>

### CHECK-2. A failed error verification fails the run; a violated warning fails nothing and changes no row

Every column and dataset verification may declare `severity` inside its
operation payload: exactly `error` or `warning`, defaulting to `error`. A
failed `error` verification fails execution and reports its stable
specification path, failure count, and representative offending keys. A
violated `warning` does not fail execution, remove or change a row, or make the
primary artifact ineligible for publication; the executor continues through
later column checks, key validation, and dataset checks, collecting warning
violations in that order.

Evidence: `schema-adsl-age-quality`, `schema-verification-log`, `schema-row-count-fractions`.
Replaces: REQ-0373, REQ-0389, REQ-0390, REQ-0406.

An `error` stops the run at the stage of [COLUMN-1](columns.md#column-1) that
evaluates it. Warning findings before a later error do not make the failed run
successful, and no accepted artifact is produced. Reporting limits may be
configurable but must not change pass or fail. Every evaluated check, a
`warning` violation included, is a row of the run report under
[CHECK-23](#check-23), which records every offending key under
[CHECK-24](#check-24) rather than applying the reporting limit.

<a id="check-3"></a>

### CHECK-3. A verification may declare a nonempty id, unique within its verification list

Every column, dataset, and intermediate verification may declare a nonempty
`id`, unique within its list. A declared ID names the check for readers and is
included in failure reports; column and dataset IDs also appear in the run
report. When the `id` is omitted, the verification's specification path
identifies the check and the report's `ID` stays missing rather than copying
the path.

Fails: `invalid_declaration`, `duplicate_identifier`.
Evidence: `schema-row-count-fractions`, `rust/crates/yamaa-adapters/tests/fixtures/static_verification_checks.json`.
Replaces: REQ-0374, REQ-0398.

A duplicate declared `id` within one dataset verification list, one column's
verification list, or one intermediate's verification list fails. The same ID
may be used in a different list.

<a id="check-4"></a>

### CHECK-4. A verification declares known fields, ordered bounds and existing columns of a compatible type

An unknown verification keyword or field is a schema failure. A `range` or
`row_count` with no bound, or with `min` greater than `max`, fails, as does a
`row_count` with `min_fraction` greater than `max_fraction` or a fraction bound
outside 0 through 1. A verification applied to an incompatible column type
fails, and so does an unknown column in `unique`, `all_or_none`, `assert`, or
`row_count.group_by`.

Fails: `invalid_declaration`, `unknown_field`.
Evidence: `rust/crates/yamaa-adapters/tests/fixtures/static_verification_checks.json`, `rust/crates/yamaa-adapters/tests/fixtures/completed_lookup_assertions.json`.
Replaces: REQ-0397, REQ-0399, REQ-0404, REQ-0405.

## Column verifications

<a id="check-5"></a>

### CHECK-5. `not_missing` passes only when every value is present and is not declared on a key; every other value check passes missing values

`not_missing` passes only when every value is non-missing. Every other value
check passes missing values; combine it with `not_missing` when absence is
invalid. A key column is never missing under
[OUTPUT-3](output.md#output-3), so a `not_missing` on a key column restates the
keys and fails validation.

Fails: `not_missing_failed`, `redundant_declaration`.
Evidence: `negative-not-missing-age`, `rust/crates/yamaa-adapters/tests/fixtures/original_not_missing.json`, `rust/crates/yamaa-adapters/tests/project_domain_submission.rs::restated_facts_and_relaxed_keys_fail_validation_before_data`, `rust/crates/yamaa-core/tests/support/diagnostic/submission.rs::VerificationNotMissingRedundant`.
Replaces: REQ-0375.

The redundant finding reports the verification's path and, in `stated_by`,
the key's position in `keys`. A column whose submission metadata resolves
`mandatory` to true on every row is checked without a `not_missing` too, and
[SUB-11](profiles/submission.md#sub-11) rejects one beside it.

<a id="check-6"></a>

### CHECK-6. `allowed_values` requires every present value to equal one listed value

`allowed_values` requires every non-missing value to equal one listed value
under its type's equality, including [Values](values.md) for
strings.

Fails: `allowed_values_failed`.
Evidence: `negative-invalid-sex`, `rust/crates/yamaa-adapters/tests/fixtures/original_column_values.json`.
Replaces: REQ-0376.

<a id="check-7"></a>

### CHECK-7. `range` requires every present numeric value to lie within its inclusive bounds

`range` requires every non-missing numeric value to be greater than or equal to
`min` and less than or equal to `max` for the supplied bounds. At least one
bound is required.

Fails: `range_failed`.
Evidence: `negative-implausible-age`, `schema-adsl-age-quality`, `schema-verification-log`.
Replaces: REQ-0377.

<a id="check-8"></a>

### CHECK-8. `max_length` bounds a string column's values by their count of Unicode scalar values

`max_length` requires every non-missing string to contain at most `max`
[Values](values.md) scalar values, and `max` is at least one. It is
declared only on a `str` column: the text a number or a temporal value renders
as is a property of [Values](values.md)'s rendering rather
than of the value.

Fails: `length_failed`, `invalid_declaration`.
Evidence: `negative-usubjid-length`, `rust/crates/yamaa-adapters/tests/fixtures/static_verification_checks.json`.
Replaces: REQ-0378, REQ-0380, REQ-0400.

The scalar count rather than bytes or UTF-16 units is also the unit
[Schema language](reference/schema-language.md) uses for `min_length`, so a
supplementary-plane scalar counts once. A length is therefore a separate check
rather than an anchored `matches` pattern; `max_length` counts scalar values
directly, not by regular-expression matching. A `max_length` whose `max` is
less than one fails.

<a id="check-9"></a>

### CHECK-9. `matches` requires every present string to match its regular expression

`matches` requires every non-missing string to match its regular expression.
[Text](text.md) owns the pattern's syntax, the engine
that reads it, and whether a match must span the whole value.

Fails: `matches_failed`.
Evidence: `negative-sex-code`, `rust/crates/yamaa-adapters/tests/fixtures/original_column_matches.json`.
Replaces: REQ-0379.

## Dataset verifications

<a id="check-10"></a>

### CHECK-10. `unique` requires the combined values of a column set other than the keys to be unique, missing values included

`unique` requires the listed columns to exist and their combined values to be
unique. Missing values participate as values; use column `not_missing` when
they are prohibited. A `unique` whose set of columns equals the set of `keys`
restates [OUTPUT-3](output.md#output-3), which already requires those values
to be present and unique, and fails validation.

Fails: `unique_failed`, `redundant_declaration`.
Evidence: `negative-unique-duplicate`, `negative-unique-repeats-keys`, `rust/crates/yamaa-adapters/tests/project_domain_submission.rs::restated_facts_and_relaxed_keys_fail_validation_before_data`.
Replaces: REQ-0381.

Write an unnamed check as `unique: [SITEID, SUBJID]`. The mapping form
`unique: {columns: [SITEID, SUBJID], id: site-subject}` also admits an
optional `severity`, and it is written only to declare one of them: a mapping
holding only `columns` restates the list and fails as `redundant_long_form`
under [SCHEMA-15](reference/schema-language.md#schema-15).

The redundant finding reports the verification's path, its columns and, in
`stated_by`, `keys`. Equality is between sets: order and a repeated column do
not matter. Any other set of columns, sharing some keys or none, is an ordinary
check.

<a id="check-11"></a>

### CHECK-11. `all_or_none` requires the listed values of each row to be all missing or all present

`all_or_none` requires at least two distinct columns. For every output row,
either every listed value must be missing or every listed value must be
non-missing.

Fails: `all_or_none_failed`, `invalid_declaration`.
Evidence: `negative-paired-dates`, `negative-sdtm-lb-unit-conversion`, `rust/crates/yamaa-adapters/tests/fixtures/original_all_or_none.json`.
Replaces: REQ-0382, REQ-0401.

An `all_or_none` with fewer than two distinct columns fails.

<a id="check-12"></a>

### CHECK-12. `assert` requires its predicate to be TRUE on every output row its when does not exempt

`assert` evaluates `require` for every output row. Every result must be
`TRUE`; `FALSE` and `UNKNOWN` fail, except on a row its `when` exempts. An
`assert` may declare `when`, which limits the rows its `require` must hold for:
when `when` is `TRUE`, `require` must be `TRUE`, and when `when` is `FALSE` or
`UNKNOWN`, the row passes and the rule does not apply.

Fails: `assert_failed`.
Evidence: `negative-adeg-rrr`, `negative-strata-mismatch`, `rust/crates/yamaa-adapters/tests/fixtures/original_assertions.json`.
Replaces: REQ-0383, REQ-0384.

With `when` declared, `when` and `require` are both evaluated for every output
row. This verification covers row-wise rules that lack a more specific
verification type. `when` selects rows here as it selects groups for
`row_count` under [CHECK-14](#check-14).

<a id="check-13"></a>

### CHECK-13. `row_count` bounds how many output rows its filter admits, as a count and as a share

`row_count` requires the output count to meet inclusive `min` and `max` bounds,
and its proportion to meet inclusive `min_fraction` and `max_fraction` bounds.
At least one bound is required, and fraction bounds must be between 0 and 1. A
group's count is how many of its constructed output rows the `filter` admits,
and a row counts only when the predicate is `TRUE`. The denominator is the
number of unfiltered rows in the group, or the whole artifact without
`group_by`.

Fails: `row_count_failed`.
Evidence: `schema-row-count-fractions`, `schema-verification-log`.
Replaces: REQ-0385, REQ-0387, REQ-1152.

`filter` is a [Formulas](formulas.md) predicate over one
constructed output row; `FALSE` and `UNKNOWN` do not count, as `filter` means
everywhere else. An empty artifact has proportion zero. `group_by` determines
the groups as [CHECK-14](#check-14) defines, and `when` selects groups to check
but does not change the denominator. The `id` is optional for either form.

Count bounds are mathematical integers in the specification rather than
runtime column values. The complete output count and the authored bounds
compare exactly; neither may be narrowed to the signed 64-bit runtime carrier.

For example, this warns when more than 5% of constructed output rows have a
missing `VAL`:

```yaml
verifications:
  - row_count:
      id: too_many_missing
      filter: "VAL IS NULL"
      max_fraction: 0.05
      severity: warning
```

<a id="check-14"></a>

### CHECK-14. Row count bounds apply to every group of the artifact's rows that its when does not exempt

`group_by` names declared columns and partitions the artifact's rows by each
value type's equality; missing values group with other missing values. Both
bounds then apply to every group, and only to the groups the artifact contains.
`row_count` may declare `when`: a group is bound when at least one of its rows
evaluates `when` to `TRUE`, and a group none of whose rows does is exempt.
Without `group_by`, the whole output is one group, bound when any row satisfies
`when`.

Fails: `row_count_failed`, `invalid_declaration`.
Evidence: `negative-adlb-two-baselines`, `negative-count-unnamed`, `schema-row-count-fractions`.
Replaces: REQ-0386, REQ-0388, REQ-0402, REQ-0403, REQ-1154.

`row_count` counts constructed output rows. [Values](values.md)
defines equality for strings, and missing values group as
[ROW-4](rows.md#row-4) partitions an input dataset. Exactly one baseline record
for each subject and parameter is a `min` and a `max` of one over the rows
whose baseline flag is `Y`, grouped by subject and parameter. The subject has
records, so a group holding no flagged record fails the `min` instead of
disappearing. A failure reports the offending groups and their counts.

A subject, visit, or parameter absent from the artifact entirely forms no
group, so no `min` here can discover it. The absent-group assertion belongs to
the derivation, where the relation defining the expected groups is readable: a
record lookup declaring `unmatched: fail` under
[Lookup and joins](lookup.md) rejects an expected group the data
cannot supply. A planning relation at the required keys gives every expected
group an input record under [ROW-7](rows.md#row-7).

`when` is a [Formulas](formulas.md) predicate over one
constructed output row. `FALSE` and `UNKNOWN` do not bind, as `filter` admits
only `TRUE` rows everywhere else. One conditional existence is at least one
baseline record for each subject and parameter that carries a baseline value:
a `min` of one over the rows whose baseline flag is set, grouped by subject and
parameter, with a `when` that selects the rows that carry a baseline value. A
group whose rows never carry one is not required to have one.

An empty or duplicated `row_count.group_by` fails. An unnamed grouped
`row_count` is valid; its failure report contains the specification path,
grouping columns, and observed counts, and the bounds are in the declaration at
that path.

## Assertions a verification cannot state

<a id="check-15"></a>

### CHECK-15. An ordered-series assertion is a derivation followed by a row-wise assertion

There is no ordered-frame assertion. An ordered-series assertion is stated as a
derivation followed by a row-wise assertion, in exactly one of three forms: the
adjacent row, a partition or its history up to the current row, or a derived
property at coarser keys or from an upstream specification.

Evidence: `negative-pr-after-cr`.
Replaces: REQ-0367, REQ-0368.

- **The adjacent row.** `row_value` under [Windows](windows.md)
  places another row's value on the row and `assert` compares the two. An
  assertion over the immediately preceding assessment rejects a partial
  response next to a complete one and passes the same fault with an assessment
  in between.
- **A partition, or its history up to the current row.** A qualified aggregate
  under [Formulas](formulas.md) reduces a source relation. Its
  `between` narrowing keeps only the records at or before a current-row value,
  and a cumulative property of collected values thus reaches the row it must be
  asserted about.
- **A derived property at coarser keys or upstream.** The specification that
  derives it is built and saved first, and the specification that asserts over
  it reads that artifact as an ordinary source, typed through the
  producing-specification link of [Inputs](inputs.md), the same split every
  other change of keys uses.

<a id="check-16"></a>

### CHECK-16. A cross-dataset link is asserted by the derivation that produces it, not by a verification

A supplemental qualifier record points to its parent domain record. The
derivation that produces the cross-dataset link asserts the link; a
verification over the finished artifact does not. An intermediate declaring
`require_match: true` rejects a value matching no record, and an intermediate
read carried by a key column or a `not_missing` column does the same.

Evidence: `sdtm-suppmh-linkage`.
Replaces: REQ-0369, REQ-0370.

The claim that every parent record has a supplemental record is not an
artifact property, and no verification here can state it. A specification that
must assert the claim derives at the parent's keys.

## The run report

<a id="check-22"></a>

### CHECK-22. Every run builds one report, which a warning requires a file for and a failed run still writes

Once a specification is admitted, every run builds one run report, whether it
succeeds or fails, and every host returns it. `output.report` names a file the
report is written to: its extension selects an [Output](output.md) profile, it
has no default, and it must differ from `output.path`. A specification
declaring any `warning` verification must declare `output.report`. Publication
writes the report file for a failed run too; the report is diagnostic output,
never an accepted artifact.

Fails: `missing_report`.
Evidence: `schema-adsl-age-quality`, `schema-verification-log`, `rust/crates/yamaa-core/tests/support/diagnostic/preflight.rs`, `python/tests/installed/installed_run_report.py::test_fatal_check_saves_only_the_report_and_keeps_original_issues`, `rust/crates/yamaa-adapters/tests/specification_sources.rs::verification_declarations_preserve_completed_prefix_and_phase_precedence`.
Replaces: REQ-0391, REQ-0396, REQ-1173, REQ-1177, REQ-1179.

A specification declaring a `warning` without `output.report` fails validation
with `missing_report`. A specification whose checks are all `error` may omit
it; its run still builds the report and the host still returns it. In a
project run each specification that runs builds its own report
([PROJECT-8](profiles/project.md#project-8)).

A specification refused while it is prepared, before its run begins, has no
report: its issues are the whole result. Once the run has begun, every
failure is a failed run with a report, a declaration finding raised when its
inputs are bound included. A run that stops before it reads an input or before
execution still reports its run, its inputs and its row templates, leaving
missing every count it never reached.

The report is built after the run's last stage, once the outcome is final:
after every verification and the output declaration checks for a run that got
that far, and after the primary artifact is encoded. A report that cannot be
built, verified or rendered refuses the build instead of recording a failure
inside itself ([CHECK-26](#check-26)). [CHECK-2](#check-2) is unchanged: a
failed run's report records the failure and makes nothing publishable.

[OUTPUT-9](output.md#output-9) rejects a report path that collides with
`output.path` or whose extension names no profile, and
[OUTPUT-10](output.md#output-10) orders its publication. Publication replaces
a stale report from an earlier run.

<a id="check-23"></a>

### CHECK-23. The report has one row per run, input dataset, row template, handler path, output column and evaluated check, in that order

The report's rows come in this order: one `run` row; one `input` row per input
dataset in declaration order; one `rows` row per row template, or one for root
`base` without row templates; one `handler` row per handler path in execution
order, zero counts included; one `column` row per output column in
`output.columns` order, for a successful run only; and one `check` row per
evaluated column or dataset verification in execution order. An intermediate's
verifications have no row, and a check a stopped run never reached has none.

Evidence: `schema-verification-log`, `schema-row-count-fractions`, `rust/crates/yamaa-adapters/tests/scalar_language.rs::failed_runs_report_the_prefix_they_reached_and_no_columns`.
Replaces: REQ-0393, REQ-1175.

`SPEC_PATH` names each row's stable specification item and is unique within
the report:

| Kind | `SPEC_PATH` |
|---|---|
| `run` | `output` |
| `input` | `input.<NAME>` |
| `rows` | `rows[<i>]`, or `base` without row templates |
| `handler` | the handler path [COLUMN-14](columns.md#column-14) counts |
| `column` | `columns.<NAME>` |
| `check` | the check's stable specification path |

The `rows` row of root `base` names the driver even when `base` is omitted
and the single declared input drives row construction. The handler rows are
exactly the handler paths the run counted under
[COLUMN-14](columns.md#column-14), in that order, the default substitutions
counted like handlers included: a `partial` date ([TIME-2](temporal.md#time-2)),
`crossed_bounds` ([TIME-9](temporal.md#time-9)) and a missing `impute` or
`precision` source ([TIME-13](temporal.md#time-13)). A run that never reached
execution counted none. Check rows keep execution order: column declaration
order first, then dataset-verification order, each check that ran and held
included. A failed stage contributes the checks it evaluated, at least one of
them `violated` at `error` severity. A check that submission metadata adds, the
presence check of [SUB-11](profiles/submission.md#sub-11) or a bound codelist
check, is a row like a declared verification, at the path of the declaration
that adds it. A value-level codelist check runs once per bound codelist; each
run is its own row, at the path that declares that codelist.

<a id="check-24"></a>

### CHECK-24. The report has fourteen fixed columns, and a kind leaves missing each column it has no value for

The report has exactly the columns of the table below, in that order and with
those [Values](values.md) types. A kind leaves missing each column the table
gives it no value for. The report carries no version and no timestamp: the
same specification and inputs give the same report bytes.

Evidence: `schema-verification-log`, `schema-parquet`, `schema-adsl-age-quality`, `schema-row-count-fractions`.
Replaces: REQ-0392, REQ-1174, REQ-1176.

| Column | Type | Value |
|---|---|---|
| `ARTIFACT` | `str` | the specification's `output.path` |
| `KIND` | `str` | `run`, `input`, `rows`, `handler`, `column` or `check` |
| `SPEC_PATH` | `str` | the row's stable specification path ([CHECK-23](#check-23)) |
| `NAME` | `str` | input dataset name; the driver dataset of a row template; handler name; column name; the check's registered verification name; missing for `run` |
| `ID` | `str` | a check's declared `id`, else missing |
| `TARGET` | `str` | the column a handler or a column check belongs to, else missing |
| `RULE` | `str` | the rule that a check's condition, or the condition a handler answers, cites under [COLUMN-15](columns.md#column-15), else missing |
| `SEVERITY` | `str` | check: `error` or `warning` |
| `OUTCOME` | `str` | run: `succeeded` or `failed`; check: `held` or `violated` |
| `CONDITION` | `str` | run: the fatal condition of a failed run; check: the failed condition |
| `COUNT` | `int` | run: output rows, missing when failed; input: records read; rows: rows emitted; handler: records answered; column: missing values; check: offending rows or groups, `0` when held |
| `TOTAL` | `int` | rows: driver records read; column: output rows; check: rows or groups evaluated; else missing |
| `KEYS` | `str` | violated check: every offending key or group as canonical JSON; else missing |
| `DETAILS` | `str` | canonical JSON: the condition context of a violated check, the input's file facts, a `str` column's longest value, else `{}` |

`NAME` of a check is its registered name under [CHECK-1](#check-1), and a
column check's `TARGET` is the column it infers there; a handler's `TARGET` is
the output column whose derivation declares it, missing for a named
intermediate's own derivations and match keys. A check's `RULE` is the rule
its condition cites, the condition the check reports when it is violated, and
is the same whether the check held or was violated; `CONDITION` names that
condition only on a violated check. A handler answers the condition its
operation raises without it, and its `RULE` is the rule that condition cites:
`unconvertible` cites the conversion rule of [VALUE-3](values.md#value-3) and
`absent` the unknown-name rule of [NAME-3](names.md#name-3). `RULE` is missing
for a handler whose operation raises no condition without it, such as a
`missing` handler of `mapping`, `cut` or `str_case`, for a default
substitution and an `impute` `fallback`, which answer no condition, and
whenever the condition cites no rule.

An input's `COUNT` is the records it decoded and its `DETAILS` is
`{"bytes": <byte length>, "path": "<path as declared>", "profile":
"<profile>"}`, with `"schema": "<producing specification path as declared>"`
for an input linked under [INPUT-10](inputs.md#input-10). `bytes` is the byte
length of the file snapshot read. For an input that reads a run-scope
producer's held serialized artifact in a project run
([PROJECT-8](profiles/project.md#project-8)), it is the length of the encoded
artifact the consumer read. It is `null` for an input supplied as a host table
and for one never read, whose `COUNT` is missing.

A row template's `TOTAL` is every record of its input dataset driver, or the
records a named intermediate driver selects; `COUNT` is the rows it emitted
after its filter. Both are missing for a row template that did not complete. A
`column` row's `COUNT` is the column's missing values over the output rows in
`TOTAL`, and a `str` column's `DETAILS` is `{"max_length": <n>}`, the longest
present value in Unicode scalar values (`0` when none is present).

A check's `COUNT` and `TOTAL` count the unit the check itself counts:

- output rows for a row-wise check;
- distinct combinations for `unique`;
- groups for a check that partitions the artifact, where an ungrouped
  `row_count` is the one group [CHECK-13](#check-13) bounds.

A row-wise check's `TOTAL` is every output row it evaluated, a row whose value
is missing included even where the missing value passes the check under
[CHECK-5](#check-5), and its `COUNT` is the rows that offend it. `COUNT` is
therefore never greater than `TOTAL`, and is `0` exactly when `OUTCOME` is
`held`. A held check leaves `KEYS` missing and `DETAILS` `{}`. A violated one
carries `KEYS` with the complete ordered sequence of offending keys or groups
rather than the bounded sample an error report may show, and `DETAILS` with
that failure's remaining context.

A violated `row_count` without `group_by` has `KEYS` `[{}]` under
[CHECK-25](#check-25) and `DETAILS` `{"count": <n>}`, with
`"denominator": <d>` added when the check declares a fraction bound: `<n>` is
the rows its `filter` admits and `<d>` every row of the artifact, as
[CHECK-13](#check-13) divides them. Only a grouped `row_count` uses `counts`:
its `DETAILS.counts` is the complete sequence of observed counts aligned with
the offending groups in `KEYS`, and `DETAILS.denominators`, present under a
fraction bound, holds the rows of each of those groups.

<a id="check-25"></a>

### CHECK-25. KEYS and DETAILS are compact ASCII JSON with names in text order

`KEYS` and `DETAILS` are compact ASCII JSON: no insignificant whitespace,
object names ordered by [Values](values.md), `null`, `true`, and `false` in
lower case, numbers in [Values](values.md)'s `str` form, and strings escaped to
ASCII by JSON's short escapes where one exists and lower-case `\\u`
hexadecimal escapes otherwise.

Evidence: `schema-verification-log`, `schema-adsl-age-quality`, `rust/crates/yamaa-adapters/src/run_report.rs::governed_projection_preserves_ascii_details_and_all_keys`.
Replaces: REQ-0394.

A scalar above `U+FFFF` is its JSON surrogate-pair escape. `KEYS` is an array
holding one object per offending row, its output keys by name, or per
offending group, its grouping columns by name; an ungrouped `row_count` has
the one empty group `[{}]`. Empty remaining context is `{}`.

<a id="check-26"></a>

### CHECK-26. The report is verified before it is written or returned, and a malformed report is an execution defect

The report is verified before any host sees it or any byte of it is written:
its columns, types and order; one row per run, input dataset, row template,
handler path, output column of a successful run, and evaluated column or
dataset check, each kind in order; a
non-missing `SPEC_PATH` unique within the report; each kind's outcome,
severity and condition vocabularies; `KEYS` on exactly the violated checks;
and `COUNT` no greater than `TOTAL` where both exist. A malformed report is an
execution defect, not a finding it can record.

Evidence: `rust/crates/yamaa-core/tests/relations_reference_scope.rs::verification_rejects_any_row_that_breaks_the_contract`.
Replaces: REQ-0395, REQ-1178.

The rows correspond one to one with what the executor evaluated: the declared
inputs, the row templates, the handler paths it counted, the declared output
columns of a successful run, and the column and dataset checks it evaluated. A
run fails with a named condition only
under [CHECK-2](#check-2) and the other rules; a report that fails its own
verification refuses the build, and no partial report is returned or written.
