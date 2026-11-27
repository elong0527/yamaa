# Output

A run publishes one artifact, the output dataset, and the run report file its
specification declares. These rules select the artifact's columns, assert its
keys, choose each file's profile, publish complete files atomically and fix
the order of the artifact's rows.

## Columns and keys

<a id="output-1"></a>

### OUTPUT-1. The artifact's columns are exactly `output.columns`, in that order

The primary artifact is the output dataset the specification derives. Its
columns are exactly the declared columns `output.columns` lists, in that
order, independent of declaration order, and its rows are the rows
[Row construction](rows.md) constructs. `output.columns` must not
repeat a column or name an undeclared column. A missing `output.columns`, a
repeated entry or an entry naming an undeclared column fails, reporting the
column name.

Fails: `duplicate_identifier`, `undeclared_column`.
Evidence: `schema-structure`, `rust/crates/yamaa-application/tests/fixtures/output_declarations.tsv::duplicate`, `rust/crates/yamaa-application/tests/fixtures/output_declarations.tsv::undeclared`, `rust/tests/host-fixtures/cases.json::an_output_column_that_no_column_declares`, `rust/tests/host-fixtures/cases.json::an_output_column_listed_twice`.

A declared column that `output.columns` does not list is internal
([SPEC-7](specification.md#spec-7)). `output.order_by` orders the artifact's
rows independent of construction order ([OUTPUT-11](#output-11)). Every
run also builds the run report of [Verification](verification.md); the report
describes the run and is neither a second derivation target nor a source
within the specification.

A repeated entry fails as `duplicate_identifier` and an entry naming an
undeclared column as `undeclared_column`, at the entry, `output.columns[<i>]`,
naming the column in `column`, and a repeated one in `identifier` too. A
submission that composes the specification refuses them the same way, at the
entry under the specification's listing,
`<section>.specs[<n>].output.columns[<i>]`
([DEFINE-7](profiles/define-xml.md#define-7)).

<a id="output-2"></a>

### OUTPUT-2. `keys` lists one or more output columns, each once

`keys` is an ordered list of columns named in `output.columns` and must name
at least one; a column must not be listed twice. Validation fails for an empty
list, an unknown or internal column, or a repeated column, and reports the
invalid name when there is one.

Fails: `duplicate_identifier`, `internal_column_in_keys`, `invalid_field_type`, `undeclared_column`.
Evidence: `negative-keys-internal`, `rust/crates/yamaa-application/tests/fixtures/output_declarations.tsv::internal-key`, `rust/crates/yamaa-application/tests/fixtures/preflight.tsv::declaration-order`, `rust/tests/host-fixtures/cases.json::a_key_that_is_not_an_output_column`, `rust/tests/host-fixtures/cases.json::a_key_that_no_column_declares`, `rust/tests/host-fixtures/cases.json::a_key_listed_twice`, `rust/tests/host-fixtures/cases.json::an_empty_key_list`.

An empty list fails as `invalid_field_type`, an unknown column as
`undeclared_column`, an internal column as `internal_column_in_keys` and a
repeated column as `duplicate_identifier`. A submission that composes the
specification refuses `keys` the same way, at the key under the
specification's listing, `<section>.specs[<n>].keys[<i>]`, and an empty list
at `<section>.specs[<n>].keys` ([DEFINE-7](profiles/define-xml.md#define-7)).

<a id="output-3"></a>

### OUTPUT-3. Every artifact row's combined key values are non-missing and unique

Once every column's lifecycle is complete, the combined key values of each row
must be non-missing and unique across the artifact. Key validation happens
before dataset verifications, which [Verification](verification.md)
runs last. String key values compare under [Values](values.md)
equality. A missing or duplicate combined key value fails, reporting the
offending rows.

Fails: `duplicate_key`, `missing_key`.
Evidence: `negative-output-duplicate`, `negative-output-missing-key`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::a_published_dataset_whose_rows_repeat_the_keys_its_specification_now_declares_fails_its_recheck`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::a_published_dataset_missing_a_key_value_fails_its_recheck`, `rust/crates/yamaa-application/tests/submission_staleness.rs::a_matching_report_does_not_admit_rows_that_repeat_the_keys_declared_after_its_run`, `rust/tests/host-fixtures/cases.json::a_composed_dataset_path_has_no_location`.

A lookup matches on the `key` its input or named intermediate declares
([LOOKUP-5](lookup.md#lookup-5)), never on these keys. The keys state this
identity once: a `not_missing` on a key column or a `unique` over exactly the
keys repeats it and fails validation ([CHECK-5](verification.md#check-5),
[CHECK-10](verification.md#check-10)). A specification without `rows` emits
one row per key combination under [ROW-8](rows.md#row-8), so a duplicate key
can come only from row templates emitting one combination more than once,
under [ROW-7](rows.md#row-7).

Sponsor variables that may be missing can still define the presentation order
and a separate uniqueness check. Declare a non-missing row identity in `keys`,
the sponsor variables in `output.order_by`, and a dataset `unique`
verification over those variables. For example, when `AENDT` is missing for an
ongoing exposure, these fields accompany the declared exposure columns:

```yaml
keys: [USUBJID, EXSEQ]
output:
  path: adex.csv
  columns: [USUBJID, EXSEQ, EXTRT, ASTDT, AENDT]
  order_by: [USUBJID, EXTRT, ASTDT, AENDT]
verifications:
  - unique: {columns: [USUBJID, EXTRT, ASTDT, AENDT], id: sponsor-keys}
```

[Ordering](values.md#value-22) sorts a missing value lowest unless the term
declares `nulls`: under the ascending terms above, a missing `AENDT` sorts
before every present end date of the same subject, treatment and start date,
and `{variable: AENDT, nulls: last}` places it after them instead.
[CHECK-10](verification.md#check-10) treats missing values as values in the
uniqueness check, so two rows with the same subject, treatment, start date and
missing end date fail that check. The missing end date remains missing.
Submission key metadata describes the identity in `keys`, as
[DEFINE-18](profiles/define-xml.md#define-18) and
[JSON-9](profiles/dataset-json.md#json-9) require; neither ordering nor the
separate uniqueness check changes it.

The same declaration verifies a dataset whose received rows violate the keys
an existing define declares while another identity holds, such as a sequence
variable when repeated measurements share a visit and date. A missing or
repeated key value is a fact about the rows, not about the derivations, and
the keys are the identity that submission metadata publishes, so a run refuses
keys the rows violate rather than publish them. Declare the identity the rows
have in `keys`, and the variables the define declared in `output.order_by` and
the `unique` verification. That verification declared with `severity: warning`
lets the run succeed ([CHECK-2](verification.md#check-2)), needs an
`output.report` ([CHECK-22](verification.md#check-22)), and its row of the
run report names each row of a repeated combination by its keys
([CHECK-24](verification.md#check-24)); with the default `error` it fails the
run. A variable that some rows miss leaves `keys` and is no longer mandatory
as a key, so a specification that must declare `mandatory`, as
`ADaM` does, declares `mandatory: false` on it
([SUB-10](profiles/submission.md#sub-10)). The `unique` verification counts a
missing value as a value ([CHECK-10](verification.md#check-10)), so it reports
a missing define-key value only when its combination repeats. A column
`not_missing` verification with `severity: warning` on each define-key
variable that is not mandatory names every row that misses it
([CHECK-5](verification.md#check-5)). A mandatory one already fails the run on
a missing value, and a `not_missing` beside it fails validation as redundant
([SUB-11](profiles/submission.md#sub-11)).

When no combination of the received variables is both non-missing and unique,
the specification derives the row identity, such as the sequence variable of
its model, and declares it in `keys`. A key is an output column
([OUTPUT-2](#output-2)) and a key column is mandatory
([SUB-10](profiles/submission.md#sub-10)), so neither a nullable variable nor
an internal column can stand in for it. Whether a key value is missing or
repeated depends on the rows, so this rule fails when a run builds them, or
when a project submission that requests a transport rechecks a published
artifact's rows at `datasets.<dataset>.keys[<n>]` or `datasets.<dataset>.keys`
([JSON-5](profiles/dataset-json.md#json-5)), never at `check()`, which reads no
data.

## Profiles

<a id="output-4"></a>

### OUTPUT-4. `output.path` is required and names the primary file

`output.path` is required, names the primary file and has no default.
`output.report` names the file the run report of
[Verification](verification.md) is written to when the specification declares
it. `output.relative_to` selects the base for both paths under
[FILE-2](reference/files.md#file-2). A missing `output.path` fails
validation, reporting the specification.

Evidence: `schema-inherited-output`, `schema-verification-log`.

`relative_to: entry` lets a shared output declaration publish separately for
each study ([LAYER-5](inheritance.md#layer-5)).

<a id="output-5"></a>

### OUTPUT-5. Each output path's extension selects the `csv` or `parquet` profile from a closed mapping

The case-insensitive extension of `output.path` selects its profile from the
closed mapping below; `output.report` selects its own the same way. An
extension outside the mapping, or none, fails
validation, reporting the path. No extension is treated as a default, and a
profile is never inferred from file contents.

Fails: `unknown_artifact_profile`.
Evidence: `negative-output-transport`, `rust/crates/yamaa-application/tests/fixtures/output_declarations.tsv::profile`, `schema-parquet`.

| Extension | Profile | Container | Fixed exactly |
|---|---|---|---|
| `.csv` | `csv` | delimited text | the bytes |
| `.parquet` | `parquet` | Parquet | schema, column and row order, values |

Serialization follows the [CSV](reference/csv.md) and [Parquet](reference/parquet.md)
profiles. The profiles own their containers and bytes, and this file owns
publication; the profiles carry the primary values and their order rather
than decide them.

SAS Transport version 5 (`.xpt`) and Dataset-JSON (`.json`) are not output
profiles: a study document's submission section writes them from published
artifacts under [SAS Transport](profiles/xpt.md) and
[Dataset-JSON](profiles/dataset-json.md), and either extension in an output
path fails like any other unmapped extension. A derivation run publishes only
the primary artifact and its declared report file.

<a id="output-6"></a>

### OUTPUT-6. A profile carries every value the language admits without changing it

The profile mapping is closed at two entries, and a container is added to it
only when it carries every value the language admits without changing one. A
profile writes Unicode scalar sequences, binary64 floats, signed 64-bit
integers, dates, local civil datetimes and times of day under the declared
column names,
without truncation, re-encoding or rounding. A value that cannot be written
under its column's mapping fails, reporting the column, the row's key and the
value.

Evidence: `schema-parquet`.

The values are the text ([VALUE-11](values.md#value-11)), numbers
([VALUE-5](values.md#value-5)) and dates, datetimes and times
([VALUE-13](values.md#value-13)) of [Values](values.md), under the names
[SPEC-2](specification.md#spec-2) admits. The `parquet`
profile carries its own types: an artifact read by another specification needs
no declaration to be understood. The `csv` profile's bytes are fixed exactly.

A profile and the language version the specification declares identify the
bytes exactly. A consumer selects the profile from its own path under
[INPUT-1](inputs.md#input-1) and reads the producer's output contract
through the producing-specification link of
[INPUT-10](inputs.md#input-10). A profile's byte and value mapping is fixed
by its producer's language version; a mapping that diverges at the same
language version must use a new profile name.

## Publication

<a id="output-7"></a>

### OUTPUT-7. A run publishes once, after the whole artifact is complete

Publication happens once, after the whole artifact is complete: after every
value's lifecycle, key validation and verification under
[Column derivation](columns.md), and after its rows are ordered.
Rows are not streamed to the target as they are constructed.

Evidence: `rust/crates/yamaa-application/tests/fixtures/diagnostics/declarative_case_predicates.json::case_predicate_multiplicity_retains_its_complete_report_and_blocks_publication`.

A conversion failure with no `unconvertible` on the result wrapper fails the
run ([VALUE-3](values.md#value-3)), and no artifact is published; the failed
run writes only its declared report file, as [OUTPUT-10](#output-10) states.

<a id="output-8"></a>

### OUTPUT-8. Each file replaces its target atomically, and a failed run leaves the target unchanged

A file becomes visible in one step. The publisher writes the complete file
into a temporary regular file, flushes and closes it so its bytes reach the
filesystem rather than a buffer, and atomically replaces the target with it.
A reader observes either the file that was there before or the complete new
one, never a prefix of it. A run that fails at any point leaves the target as
it was, removes its temporary file and produces no accepted artifact.

Fails: `publication_failed`.
Evidence: `rust/crates/yamaa-adapters/src/file_publication/unix.rs::replacing_the_parent_temporary_entry_does_not_replace_checked_bytes`, `rust/crates/yamaa-adapters/src/file_publication/unix.rs::failed_cleanup_preserves_the_operation_error_and_foreign_entries`, `rust/crates/yamaa-application/src/file_define/publication_failure_tests.rs::later_publication_failure_preserves_prior_bytes_original_cause_and_stopped_remainder`, `rust/crates/yamaa-application/src/file_application.rs::a_failure_or_a_cancellation_between_two_partitions_of_the_output_publishes_nothing`, `rust/crates/yamaa-adapters/src/file_publication/unix.rs::a_new_file_takes_0666_less_the_umask`, `rust/crates/yamaa-adapters/src/file_publication/unix.rs::a_replaced_file_keeps_its_permission_bits`, `rust/crates/yamaa-adapters/src/file_publication/unix.rs::the_staging_directory_stays_private_under_any_umask`.

A failed atomic replacement fails and reports the target; the run produces no
artifact and the previous one is unchanged. A write the operating system
refuses, such as one into a read-only directory, reports its reason as written
by the operating system in the refusal's `cause` context. A target the save cannot reach, in
a directory that does not exist, at a directory or through a link, fails as
[FILE-11](reference/files.md#file-11) states.

The temporary file is regular and stays in the target's own filesystem, in the
target's directory or a private staging directory beneath it. A private
staging directory belongs to the publishing user and grants no access to other
users. The publisher retains its directory descriptor, so replacement of its
entry in the target's parent cannot substitute the checked candidate.
Temporary names or their private namespaces do not collide with the target or
another run's candidate. The final replacement remains atomic.

On Unix the published file's mode does not depend on the staging directory. A
new file gets the mode `open(2)` gives a file it creates in the target's
directory with mode 0666: 0666 less the process umask, where that directory
has no default ACL. A file that replaces a target keeps the target's
permission bits, the 0777 bits of its mode. The temporary file has that mode
before it replaces the target.

Publishers coordinate writes through an exclusive lock on the selected parent
directory, and a lock refusal leaves the target untouched. A save holds the
lock of each directory it publishes into from the first file it stages there
until the save ends. An explicit save intentionally replaces the current
target; it is not a conditional update against the target that existed when
the publisher was created. The last successful replacement supplies the
visible artifact.

If external interference or a filesystem failure prevents temporary cleanup,
the failure preserves the original operation error and reports the cleanup
failure and the staging location; it does not claim that no residue remains.
Cleanup does not remove an entry whose ownership is unconfirmed or whose
observed identity has changed. The failed publication still produces no
accepted artifact and leaves the target untouched.

## The report file

<a id="output-9"></a>

### OUTPUT-9. The primary file and the report file have distinct paths, each selecting its own profile

`output.path` and `output.report` must be different paths; naming one file
for both fails validation, reporting both fields. Each path's extension
selects its profile under [OUTPUT-5](#output-5) independently of the other:
the primary file may be Parquet while the report is CSV, or the reverse. A
report extension the mapping does not name fails validation as it does for
`output.path`.

Fails: `artifact_path_collision`, `unknown_artifact_profile`.
Evidence: `rust/crates/yamaa-core/tests/fixtures/diagnostics/preflight.json::the_report_file_is_not_the_output_file`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/preflight.json::the_report_file_extension_names_a_known_profile`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/project_scope.json::outside_the_sdtm_scope_a_collision_is_not_part_of_the_run`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/project_scope.json::two_run_scope_files_at_one_location_refuse_the_whole_run`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/producer_links.json::a_consumer_may_not_write_its_producers_artifact_as_its_output`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/producer_links.json::a_consumer_may_not_write_its_producers_artifact_as_its_report`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/preflight.json::check_refuses_an_output_path_that_names_its_input`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/preflight.json::a_run_refuses_an_output_path_that_names_its_input_in_another_spelling_and_writes_nothing`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/preflight.json::check_refuses_a_report_path_that_names_its_input_in_another_spelling`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/preflight.json::a_failing_run_refuses_a_report_path_that_names_its_input_and_writes_nothing`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/preflight.json::an_output_may_not_name_the_specification_an_input_links_as_its_schema`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/preflight.json::an_output_may_not_name_a_layer_the_specification_reads`, `rust/crates/yamaa-adapters/src/file_resources.rs::names_match_by_the_case_table_under_each_policy`, `rust/crates/yamaa-adapters/src/file_resources.rs::case_only_spellings_collide_on_windows_and_macos_and_respell_on_windows`, `rust/crates/yamaa-adapters/src/file_resources.rs::the_utf16_policy_replays_the_recorded_windows_classes`, `rust/crates/yamaa-application/tests/file_diagnostic_origins.rs::check_refuses_an_output_that_differs_from_its_input_only_by_case`.

Two spellings that resolve to the same file collide as well; the comparison
uses the resolved locations without requiring either file or its parent to
exist. On Windows and macOS two locations that differ only in case are one
file, so `DM.csv` collides with `dm.csv` there even on a case-sensitive volume:
they compare by the simple uppercase of Unicode 15.0.0 without the dotless i
and the long s. Windows compares per UTF-16 code unit, as its ordinal
comparison and NTFS do, and applies only the rows of that table its ordinal
comparison applies: a letter outside the Basic Multilingual Plane compares
exactly there, and so do the 215 code units that Windows keeps apart from
their simple uppercase, such as final sigma U+03C2 against U+03A3, the
titlecase digraph U+01C5 against U+01C4 and U+01C6, Cherokee lowercase and
Georgian Mtavruli. macOS compares per Unicode scalar by the whole table, so
such letters fold there: U+10428 collides with U+10400 and U+03C2 with U+03A3
on macOS. macOS compares only case, by that table: a
difference that a case-insensitive APFS volume also ignores, such as Unicode
normalization (U+00E9 against `e` and U+0301) or a folding beyond simple
uppercase (the Kelvin sign U+212A against `k`, the long s against `s`, U+1E9E
against U+00DF), is not detected as one location. On other platforms locations
compare exactly. A specification does not write the files it reads: neither
path may name the `output.path` or `output.report` location of a producer an
input links under [INPUT-10](inputs.md#input-10), reporting the producer's
field and this specification's, nor the location of an input's own `path` or
`schema`, reporting the output field and the input's, nor any metadata file the
specification reads: its own file, a layer, a dictionary, or a project,
function or codelist file of its project, reporting the output field alone.
`check` decides each without reading data, and a run refused this way opens no
file for writing, its report included. In a project run, no two of the
`output.path` and `output.report` paths of the specifications in its run scope
may name one file ([PROJECT-12](profiles/project.md#project-12)).

<a id="output-10"></a>

### OUTPUT-10. The report file publishes before the primary file, and a failed run publishes only its report

A successful run renders and validates every file it publishes before
touching any target, then replaces each under [OUTPUT-8](#output-8): the
declared report file first, then the primary artifact. Warning violations do
not prevent publication. A failed run publishes no primary artifact and
replaces only its declared report file; it is the only file a failed run
writes, and writing it accepts nothing.

Evidence: `schema-verification-log`, `python/tests/installed/installed_run_report.py::test_fatal_check_saves_only_the_report_and_keeps_original_issues`, `rust/crates/yamaa-application/src/file_application.rs::actual_late_failures_retain_original_error_entered_prefix_and_retry`, `rust/crates/yamaa-application/src/file_application.rs::failed_build_report_keeps_original_failure_and_fresh_retry_prefix`.

A successful publication therefore never exposes a new primary artifact
without its completed report already visible. The run report
[Verification](verification.md) fixes is diagnostic output, not one of the
artifacts of [OUTPUT-1](#output-1); a run that declares no report file
publishes only its primary artifact.

The report is rendered when the build completes, so a report that cannot be
rendered refuses the build ([CHECK-26](verification.md#check-26)) before
publication. Failure to replace the report file is an output failure that
leaves every other target untouched. If replacing the primary artifact fails
after the report was replaced, the publication fails and the prior primary
artifact remains; the complete report may remain as the record of the
completed candidate run. Atomic replacement is guaranteed per file, not
simultaneously across paths.

## Row order

<a id="output-11"></a>

### OUTPUT-11. `output.order_by` orders the artifact's rows once, after every verification

`output.order_by` is optional and declares the order the artifact's rows are
presented in. An artifact whose specification omits it keeps construction
order: row-template order, and base-record order or first-occurrence group
order within each row template. Ordering is a presentation step that runs
once, after the lifecycle, key validation and every verification; it never
changes whether a run passes or warns, and evaluation is unchanged.

Evidence: `schema-ordering`, `adam-adae-review-order`.

The dependency order of the [Column derivation](columns.md), a
window's partitions and the neighbours `row_value` reads are fixed before
ordering, and each uses construction order for its own tie-break.
[Values](values.md) defines order terms, their direction and
missing placement, and the tie-break of rows equal on every term.

<a id="output-12"></a>

### OUTPUT-12. Each `output.order_by` term names a different declared column

Every `output.order_by` term must name a declared column, output or internal,
and no variable may be repeated. Validation reports an unknown or repeated
term. A qualified source variable is not a declared column and has no value on
a completed row to order by.

Fails: `duplicate_order_term`, `undeclared_column`.
Evidence: `negative-order-repeated`, `negative-order-unknown-column`, `adam-adae-review-order`.

A working value the artifact does not publish is admissible, such as a numeric
ordinal beside the text it labels, or a rank.

## Kept rows

<a id="output-13"></a>

### OUTPUT-13. `output.filter` keeps the completed rows it holds for, before the keys and every verification

`output.filter` is an optional predicate that keeps the artifact rows whose
result is `TRUE`. It reads the completed rows: it evaluates once every
column's lifecycle is complete, after the window pass, and before every
column verification, key validation, every dataset verification and
ordering, which see only the kept rows. It names declared columns, output or
internal, alone; a name that is not a declared column fails validation,
reporting it.

Fails: `undeclared_column`.
Evidence: `schema-output-filter`, `schema-output-filter-checks`, `negative-output-filter-unknown-column`, `negative-output-filter-division`, `rust/crates/yamaa-core/src/dataset/partitioned_execution.rs::an_output_filter_keeps_rows_before_every_check_over_any_partitions`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/preflight.json::an_output_filter_names_only_declared_columns`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/declarative_self.json::a_dataset_check_reads_self_over_the_kept_rows_when_the_output_filter_drops_a_donor`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/declarative_self.json::a_dataset_check_reselects_a_self_read_whose_donor_the_output_filter_moved`.

`FALSE` and `UNKNOWN` drop the row, as every `filter` treats them
([Rules](README.md#terminology)). A dropped row was still built: a window
numbered it and a `SELF` lookup read it, so a kept row's values are those of
the whole dataset. A dataset verification that reads a `SELF` intermediate
selects over the kept rows, the rows it checks, so it can select another
record than a column read over the whole dataset. A column verification, a
submission's metadata check among them, judges the kept rows alone, so with `output.filter` the column
verifications run once every column is complete rather than as each column
completes. A fault evaluating the filter on a row is reported as a row
template's `filter` reports one ([ROW-3](rows.md#row-3)), before any column
or dataset check runs. A part of a dataset
split by a column keeps the whole dataset's sequence numbers this way, where a
row template's `filter`, which evaluates before the window pass
([ROW-6](rows.md#row-6)), would number its part's rows again from 1.

A qualified source variable or a lookup field is not a declared column and
has no value on a completed row, so naming one fails as `undeclared_column`
at `output.filter`, naming it in `column`, as an `output.order_by` term does
([OUTPUT-12](#output-12)). A predicate's grammar and its comparisons of
declared types are checked before any record is read, as every predicate's
are ([FORMULA-5](formulas.md#formula-5)).

The keys must be non-missing and unique over the kept rows
([OUTPUT-3](#output-3)), and a dataset verification counts the kept rows
alone. In the run report ([CHECK-24](verification.md#check-24)) the run's
`COUNT`, each `column` row's `TOTAL` and each row-wise check's `TOTAL` count
the kept rows, while a `rows` row's `COUNT` stays the rows its row template
emitted, before the output filter. A filter that keeps no row publishes an
artifact of no rows. A layer that inherits a complete specification and adds only
`output.path` and `output.filter` publishes one part of its parent's dataset
([LAYER-5](inheritance.md#layer-5)).
