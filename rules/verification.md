# Verification

Verifications assert properties of completed values: column verifications over
one converted column, dataset verifications over the completed output. A failed
`error` verification fails the run; a violated `warning` is recorded in the
warning log, and a declared verification log records every check the run
evaluated.

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

Evidence: `adam-adsl-age-quality`, `schema-verification-log`, `schema-row-count-fractions`.
Replaces: REQ-0373, REQ-0389, REQ-0390, REQ-0406.

An `error` stops the run at the stage of [COLUMN-1](columns.md#column-1) that
evaluates it. Warning findings before a later error do not make the failed run
successful, and no accepted artifact is produced. Reporting limits may be
configurable but must not change pass or fail. A `warning` violation instead
follows [CHECK-17](#check-17) to [CHECK-19](#check-19) and records every
offending key under [CHECK-18](#check-18) rather than applying the reporting
limit.

<a id="check-3"></a>

### CHECK-3. A verification may declare a nonempty id, unique within its verification list

Every column, dataset, and intermediate verification may declare a nonempty
`id`, unique within its list. A declared ID names the check for readers and is
included in failure reports; column and dataset IDs also appear in the governed
logs. When the `id` is omitted, the verification's specification path
identifies the check and the `VERIFICATION_ID` field stays missing rather than
copying the path.

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
Evidence: `negative-implausible-age`, `adam-adsl-age-quality`, `schema-verification-log`.
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
verification over the finished artifact does not. An intermediate without
`no_match` rejects a value matching no record, and an intermediate read carried
by a key column or a `not_missing` column does the same.

Evidence: `sdtm-suppmh-linkage`.
Replaces: REQ-0369, REQ-0370.

The claim that every parent record has a supplemental record is not an
artifact property, and no verification here can state it. A specification that
must assert the claim derives at the parent's keys.

## The warning log

<a id="check-17"></a>

### CHECK-17. A specification declaring a warning declares a warning log, which every successful run produces

A specification declaring any warning must declare `output.warning_log`. Its
path must differ from `output.path` and selects an
[Output](output.md) profile by the same closed
extension mapping. The log is built only for a successful execution, after all
verifications and before publication, and a successful run produces it even
when no warning is violated; the empty case is a header-only dataset.

Fails: `missing_warning_log`.
Evidence: `adam-adsl-age-quality`, `rust/crates/yamaa-core/tests/support/diagnostic/preflight.rs`.
Replaces: REQ-0391, REQ-0396.

Publication thus replaces a stale non-empty log from an earlier run. A
warning-log build or serialization failure fails the run and prevents
primary-artifact publication. [Output](output.md)
defines how a runner publishes the completed pair.

<a id="check-18"></a>

### CHECK-18. The warning log has one row per violated warning declaration, in ten fixed columns

The warning log is version 1.0 and has exactly the columns below, in this
order and with these [Values](values.md) types. It has one
row per violated warning declaration, keyed by `SPEC_PATH`, in execution order:
column declaration order first, then dataset-verification order.
`FAILURE_COUNT` is positive, and `OFFENDING_KEYS` is the complete ordered
sequence, not the bounded sample an error report may show.

Evidence: `adam-adsl-age-quality`, `schema-verification-log`, `schema-row-count-fractions`.
Replaces: REQ-0392, REQ-0393, REQ-0395.

| Column | Type | Value |
|---|---|---|
| `LOG_VERSION` | `str` | `1.0` |
| `ARTIFACT` | `str` | the specification's `output.path` |
| `SEVERITY` | `str` | `warning` |
| `CONDITION` | `str` | the stable failed condition |
| `REQUIREMENT` | `str` | the numbered requirement defining the check |
| `SPEC_PATH` | `str` | the violated check's stable specification path |
| `VERIFICATION_ID` | `str` | its declared ID, or missing when it has none |
| `FAILURE_COUNT` | `int` | the total number of offending rows or groups |
| `OFFENDING_KEYS` | `str` | every offending key or group as canonical JSON |
| `DETAILS` | `str` | remaining condition context as canonical JSON |

For grouped `row_count`, `DETAILS.counts` is the complete sequence of observed
counts aligned with those groups.

The log is verified before it becomes an artifact. These properties of the log
must hold:

- its columns, types, and order;
- non-missing and unique `SPEC_PATH`;
- fixed version and severity;
- positive count;
- complete key sequence;
- one-to-one correspondence with the executor's warning findings.

A malformed log is an execution defect, not a warning that can be logged
inside itself.

<a id="check-19"></a>

### CHECK-19. The warning log's two JSON fields are compact ASCII JSON with names in text order

The two JSON fields, `OFFENDING_KEYS` and `DETAILS`, are compact ASCII JSON:
no insignificant whitespace, object names ordered by
[Values](values.md), `null`, `true`, and `false` in lower case,
numbers in [Values](values.md)'s `str` form, and strings
escaped to ASCII by JSON's short escapes where one exists and lower-case `\\u`
hexadecimal escapes otherwise.

Evidence: `schema-verification-log`, `adam-adsl-age-quality`.
Replaces: REQ-0394.

A scalar above `U+FFFF` is its JSON surrogate-pair escape. The empty key
sequence is `[]` and empty remaining context is `{}`.

## The verification log

<a id="check-20"></a>

### CHECK-20. A declared verification log records every check the run evaluated, for failed runs too

`output.verification_log` names a governed sidecar recording the outcome of
every verification the specification declares, held or violated, whatever its
severity. A run that declares the field always produces the log, violations or
not, for a failed run as well as a successful one. The log is diagnostic output
rather than one of [Output](output.md)'s artifacts:
a failed `error` verification still produces no accepted artifact.

Evidence: `schema-verification-log`, `python/tests/installed/installed_public_contract.py::test_failed_verification_retains_owned_diagnostic_log`, `rust/crates/yamaa-adapters/tests/specification_sources.rs::verification_declarations_preserve_completed_prefix_and_phase_precedence`.
Replaces: REQ-1173, REQ-1177, REQ-1179.

A specification whose checks are all `error` may declare it, and one declaring
warnings may omit it. [Output](output.md) selects
its profile from its path by the same closed extension mapping and requires
that path to differ from `output.path` and `output.warning_log`. A path that
collides with either, or whose extension names no profile, fails validation
under [OUTPUT-9](output.md#output-9). Publication replaces a stale
log from an earlier run.

The log records a failure but does not make the run publishable, and
[CHECK-2](#check-2) is unchanged. Its rows are the checks the run evaluated, in
execution order. The stage that failed contributes the checks it evaluated, at
least one of them `violated` at `error` severity, and a check a stopped run
never reached has no row. The log states what was checked and nothing more.

<a id="check-21"></a>

### CHECK-21. The verification log has one row per declared check, in thirteen fixed columns

The verification log is version 1.0 and has exactly the columns below, in this
order and with these [Values](values.md) types. It has one
row for every declared check rather than one per violation, in execution order
exactly as [CHECK-18](#check-18) orders the warning log. `SPEC_PATH` is
its key and its join to the warning log. `EVALUATED_COUNT` and `FAILURE_COUNT`
count the unit the check itself counts.

Evidence: `schema-verification-log`, `schema-row-count-fractions`, `rust/crates/yamaa-adapters/tests/project_domain_submission.rs::date_and_time_data_types_check_each_completed_value_against_their_form`.
Replaces: REQ-1174, REQ-1175, REQ-1176, REQ-1178.

| Column | Type | Value |
|---|---|---|
| `REPORT_VERSION` | `str` | `1.0` |
| `ARTIFACT` | `str` | the specification's `output.path` |
| `SPEC_PATH` | `str` | the check's stable specification path |
| `VERIFICATION_ID` | `str` | its declared ID, or missing when it has none |
| `CHECK` | `str` | the registered verification name |
| `TARGET` | `str` | the verified column, or missing for a dataset verification |
| `REQUIREMENT` | `str` | the numbered requirement defining the check |
| `SEVERITY` | `str` | `error` or `warning` |
| `OUTCOME` | `str` | `held` or `violated` |
| `CONDITION` | `str` | the stable failed condition, or missing when `OUTCOME` is `held` |
| `EVALUATED_COUNT` | `int` | rows, or groups for a grouped check, the check examined |
| `FAILURE_COUNT` | `int` | offending rows or groups; `0` when `OUTCOME` is `held` |
| `DETAILS` | `str` | remaining condition context as canonical JSON |

A check that ran and held is a row. `SPEC_PATH` is non-missing and unique
within the log; the warning log's row for the same path carries the complete
`OFFENDING_KEYS` evidence [CHECK-18](#check-18) requires, and offending keys
stay out of this log. `CHECK` is the verification's registered name under
[CHECK-1](#check-1), and `TARGET` is the column a column verification infers
under [CHECK-1](#check-1), missing for a dataset verification. A check that
submission metadata implies is recorded under the name of the verification it
fails as: a mandatory column's as `not_missing`
([SUB-11](profiles/submission.md#sub-11)), a declared length's as `max_length`
([SUB-8](profiles/submission.md#sub-8)) and a codelist binding's as
`allowed_values`. The ISO 8601 form of a date or time data type
([SUB-26](profiles/submission.md#sub-26)) has no registered verification and is
recorded as `data_type`.

The unit `EVALUATED_COUNT` and `FAILURE_COUNT` count is:

- output rows for a row-wise check;
- distinct combinations for `unique`;
- groups for a check that partitions the artifact, where an ungrouped
  `row_count` is the one group [CHECK-13](#check-13) bounds.

`FAILURE_COUNT` is therefore never greater than `EVALUATED_COUNT`, and is `0`
exactly when `OUTCOME` is `held`. A held check leaves `CONDITION` missing and
`DETAILS` `{}`. A violated one carries the stable condition its failure reports
and that failure's remaining context, encoded by [CHECK-19](#check-19)
unchanged rather than by a second encoding.

The log is verified before it is written:

- its columns, types, and order;
- its fixed version;
- non-missing and unique `SPEC_PATH`;
- `OUTCOME` exactly `held` or `violated`;
- the count relationship above;
- one-to-one correspondence with the checks the executor evaluated, including
  agreement with the warning log about every warning that log carries.

A malformed log is an execution defect, not a finding that can be recorded
inside itself.
