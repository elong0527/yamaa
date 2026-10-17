# Output

A run publishes one artifact, the output dataset, and the sidecars its
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
Evidence: `schema-structure`, `rust/crates/yamaa-adapters/tests/fixtures/output_declarations.tsv::duplicate`, `rust/crates/yamaa-adapters/tests/fixtures/output_declarations.tsv::undeclared`.
Replaces: REQ-0062, REQ-0193, REQ-0210, REQ-0234, REQ-1047.

A declared column that `output.columns` does not list is internal
([SPEC-7](specification.md#spec-7)). `output.order_by` orders the artifact's
rows independent of construction order ([OUTPUT-11](#output-11)). A
specification may also produce the governed warning log of
[Verification](verification.md); that log reports the run and is
neither a second derivation target nor a source within the specification.

<a id="output-2"></a>

### OUTPUT-2. `keys` lists one or more output columns, each once

`keys` is an ordered list of columns named in `output.columns` and must name
at least one; a column must not be listed twice. Validation fails for an empty
list, an unknown or internal column, or a repeated column, and reports the
invalid name when there is one.

Fails: `duplicate_identifier`, `internal_column_in_keys`, `invalid_field_type`, `undeclared_column`.
Evidence: `negative-keys-internal`, `rust/crates/yamaa-adapters/tests/fixtures/output_declarations.tsv::internal-key`, `rust/crates/yamaa-adapters/tests/fixtures/preflight.tsv::declaration-order`.
Replaces: REQ-0220.

An empty list fails as `invalid_field_type`, an unknown column as
`undeclared_column`, an internal column as `internal_column_in_keys` and a
repeated column as `duplicate_identifier`.

<a id="output-3"></a>

### OUTPUT-3. Every artifact row's combined key values are non-missing and unique

Once every column's lifecycle is complete, the combined key values of each row
must be non-missing and unique across the artifact. Key validation happens
before dataset verifications, which [Verification](verification.md)
runs last. String key values compare under [Values](values.md)
equality. A missing or duplicate combined key value fails, reporting the
offending rows.

Fails: `duplicate_key`, `missing_key`.
Evidence: `negative-output-duplicate`, `negative-output-missing-key`.
Replaces: REQ-0221, REQ-0240.

Key order is significant to [Lookup and joins](lookup.md), which
joins on the output keys a right side also carries; those applicable keys are
used for enrichment and do not change the identity asserted here. A
specification without `rows` emits one row per key combination under
[ROW-8](rows.md#row-8), so a duplicate key can come only from
row templates emitting one combination more than once, under
[ROW-7](rows.md#row-7).

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

[Ordering](values.md#value-22) places missing values last by
default. [CHECK-10](verification.md#check-10) treats missing values
as values in the uniqueness check, so two rows with the same subject,
treatment, start date and missing end date fail that check. The missing end
date remains missing. Submission key metadata describes the identity in
`keys`, as [DEFINE-18](profiles/define-xml.md#define-18) and
[JSON-9](profiles/dataset-json.md#json-9) require; neither ordering nor
the separate uniqueness check changes it.

## Profiles

<a id="output-4"></a>

### OUTPUT-4. `output.path` is required and names the primary file

`output.path` is required, names the primary file and has no default.
`output.warning_log` and `output.verification_log` name the sidecars of
[Verification](verification.md) when the specification declares
them. `output.relative_to` selects the base for all three paths under
[FILE-2](reference/files.md#file-2). A missing `output.path` fails
validation, reporting the specification.

Evidence: `schema-inherited-output`, `schema-verification-log`.
Replaces: REQ-0715, REQ-0759.

`relative_to: entry` lets a shared output declaration publish separately for
each study ([LAYER-5](inheritance.md#layer-5)).

<a id="output-5"></a>

### OUTPUT-5. Each output path's extension selects the `csv` or `parquet` profile from a closed mapping

The case-insensitive extension of `output.path` selects its profile from the
closed mapping below; `output.warning_log` and `output.verification_log`
select theirs the same way. An extension outside the mapping, or none, fails
validation, reporting the path. No extension is treated as a default, and a
profile is never inferred from file contents.

Fails: `unknown_artifact_profile`.
Evidence: `negative-output-transport`, `rust/crates/yamaa-adapters/tests/fixtures/output_declarations.tsv::profile`, `schema-parquet`.
Replaces: REQ-0195, REQ-0716, REQ-0760, REQ-1233, REQ-1235, REQ-1237.

| Extension | Profile | Container | Fixed exactly |
|---|---|---|---|
| `.csv` | `csv` | delimited text | the bytes |
| `.parquet` | `parquet` | Parquet | schema, column and row order, values |

Serialization follows the [CSV](reference/csv.md) and [Parquet](reference/parquet.md)
profiles. The profiles own their containers and bytes, and this file owns
publication; the profiles carry the primary values and their order rather
than decide them.

SAS Transport v5 (`.xpt`) is not an output profile. Dataset-JSON is generated
by a study document under [Dataset-JSON](profiles/dataset-json.md); `.json`
is not a specification output profile and fails like any other unmapped
extension. A derivation run publishes only the primary artifact and its
declared sidecars; conversion to a transport container is a separate
packaging step outside the language.

<a id="output-6"></a>

### OUTPUT-6. A profile carries every value the language admits without changing it

The profile mapping is closed at two entries, and a container is added to it
only when it carries every value the language admits without changing one. A
profile writes Unicode scalar sequences, binary64 floats, signed 64-bit
integers, dates and local civil datetimes under the declared column names,
without truncation, re-encoding or rounding. A value that cannot be written
under its column's mapping fails, reporting the column, the row's key and the
value.

Evidence: `schema-parquet`.
Replaces: REQ-0719, REQ-0720, REQ-0721, REQ-0763, REQ-1234.

The values are those of [Values](values.md),
[Values](values.md) and [Values](values.md),
under the names [SPEC-2](specification.md#spec-2) admits. The `parquet`
profile carries its own types: an artifact read by another specification needs
no declaration to be understood. The `csv` profile's bytes are fixed exactly.

A profile and the specification's `schema_version` identify the bytes exactly.
A consumer selects the profile from its own path under
[INPUT-1](inputs.md#input-1) and reads the producer's output contract
through the producing-specification link of
[INPUT-10](inputs.md#input-10). A profile's byte and value mapping is fixed
by its producer's `schema_version`; a mapping that diverges at the same schema
version must use a new profile name.

## Publication

<a id="output-7"></a>

### OUTPUT-7. A run publishes once, after the whole artifact is complete

Publication happens once, after the whole artifact is complete: after every
value's lifecycle, key validation and verification under
[Column derivation](columns.md), and after its rows are ordered.
Rows are not streamed to the target as they are constructed.

Evidence: `rust/crates/yamaa-adapters/tests/execution_declarative_case_predicates.rs::case_predicate_multiplicity_retains_its_complete_report_and_blocks_publication`.
Replaces: REQ-0239, REQ-0755.

A conversion failure with no `unconvertible` on the result wrapper fails the
run ([VALUE-3](values.md#value-3)), and nothing is published.

<a id="output-8"></a>

### OUTPUT-8. Each file replaces its target atomically, and a failed run leaves the target unchanged

A file becomes visible in one step. The publisher writes the complete file
into a temporary regular file, flushes and closes it so its bytes reach the
filesystem rather than a buffer, and atomically replaces the target with it.
A reader observes either the file that was there before or the complete new
one, never a prefix of it. A run that fails at any point leaves the target as
it was, removes its temporary file and produces no accepted artifact.

Fails: `publication_failed`.
Evidence: `rust/crates/yamaa-adapters/src/file_publication.rs::replacing_the_parent_temporary_entry_does_not_replace_checked_bytes`, `rust/crates/yamaa-adapters/src/file_publication.rs::failed_cleanup_preserves_the_operation_error_and_foreign_entries`, `rust/crates/yamaa-adapters/src/file_define/publication_failure_tests.rs::later_publication_failure_preserves_prior_bytes_original_cause_and_stopped_remainder`.
Replaces: REQ-0752, REQ-0753, REQ-0754, REQ-0764.

A failed atomic replacement fails and reports the target; the run produces no
artifact and the previous one is unchanged.

The temporary file is regular and stays in the target's own filesystem, in the
target's directory or a private staging directory beneath it. A private
staging directory belongs to the publishing user and grants no access to other
users. The publisher retains its directory descriptor, so replacement of its
entry in the target's parent cannot substitute the checked candidate.
Temporary names or their private namespaces do not collide with the target or
another run's candidate. The final replacement remains atomic.

Publishers coordinate writes through an exclusive lock on the selected parent
directory, and a lock refusal leaves the target untouched. An explicit save
intentionally replaces the current target; it is not a conditional update
against the target that existed when the publisher was created. The last
successful replacement supplies the visible artifact.

If external interference or a filesystem failure prevents temporary cleanup,
the failure preserves the original operation error and reports the cleanup
failure and the staging location; it does not claim that no residue remains.
Cleanup does not remove an entry whose ownership is unconfirmed or whose
observed identity has changed. The failed publication still produces no
accepted artifact and leaves the target untouched.

## Sidecars

<a id="output-9"></a>

### OUTPUT-9. The primary file and its sidecars have distinct paths, each selecting its own profile

`output.path`, `output.warning_log` and `output.verification_log` must be
three different paths; reusing one path for two of them fails validation,
reporting both fields. Each path's extension selects its profile under
[OUTPUT-5](#output-5) independently of the others: the primary file may be
Parquet while its log is CSV, or the reverse. A sidecar extension the mapping
does not name fails validation as it does for `output.path`.

Fails: `artifact_path_collision`, `unknown_artifact_profile`.
Evidence: `rust/crates/yamaa-core/tests/support/diagnostic/preflight.rs`, `python/tests/installed/installed_verification_logs.py::test_sidecar_collisions_and_unknown_profiles_precede_study_access`.
Replaces: REQ-0756, REQ-1180.

<a id="output-10"></a>

### OUTPUT-10. Logs publish before the primary file, and a failed run publishes only its verification log

A successful run renders and validates every file it publishes before
touching any target, then replaces each under [OUTPUT-8](#output-8): the
verification log first, then the warning log, then the primary artifact. Warning violations do
not prevent publication. A failed run publishes neither a primary artifact nor
a warning log and replaces only its verification log; it is the only file a
failed run writes, and writing it accepts nothing.

Evidence: `schema-verification-log`, `python/tests/installed/installed_verification_logs.py::test_fatal_checks_publish_only_verification_and_keep_original_issues`, `rust/crates/yamaa-adapters/src/file_application.rs::actual_late_failures_retain_original_error_entered_prefix_and_retry`, `rust/crates/yamaa-adapters/src/file_application.rs::failed_build_ancillary_log_keeps_original_failure_and_fresh_retry_prefix`.
Replaces: REQ-0757, REQ-0758, REQ-1181.

A successful publication therefore never exposes a new primary artifact
without its completed logs already visible. The verification log
[Verification](verification.md) fixes is diagnostic output, not one
of the artifacts of [OUTPUT-1](#output-1).

Failure to render or replace the warning log is an output failure, not a
warning, and the primary target is not touched. Failure to render or replace
the verification log is an output failure that leaves every other target
untouched. If replacing the primary artifact fails after the log was replaced,
the publication fails and the prior primary artifact remains; the complete
log may remain as the record of the completed candidate run. Atomic
replacement is guaranteed per file, not simultaneously across paths.

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
Replaces: REQ-0222, REQ-0226.

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
Replaces: REQ-0223, REQ-0224.

A working value the artifact does not publish is admissible, such as a numeric
ordinal beside the text it labels, or a rank.
