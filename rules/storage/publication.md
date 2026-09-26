---
id: storage/publication
title: Artifact publication
status: normative
---

# Artifact publication

## Requirements

### Dependency execution

<a id="req-0062"></a>

**REQ-0062.** `output.columns` selects and orders artifact columns
independently of declaration order, and `output.order_by` orders the
artifact's rows independently of the construction order [Row construction](../execution/rows.md) defines.
This contract owns column selection; [Ordering](../execution/ordering.md) owns artifact row order.

### The artifact

<a id="req-0193"></a>

**REQ-0193.** The **primary artifact** is the dataset this specification derives.
Its columns are exactly the declared columns listed by `output.columns`, in
that order. Its rows are the rows [Row construction](../execution/rows.md) constructs. A specification may
also produce [Verification](../execution/verification.md)'s governed warning sidecar. That log reports
the run, not a second derivation target or source within this specification.

<a id="req-0195"></a>

**REQ-0195.** Serialization follows the [CSV](csv.md) and [Parquet](parquet.md) profiles. `output.path` names the primary
file, and `output.warning_log` and `output.verification_log`, when
present, name [Verification](../execution/verification.md)'s sidecars. Each
extension selects `parquet` or `csv`. The format profiles own their containers and bytes; this contract owns
publication. Everything below concerns the primary values and their order,
which the profiles carry rather than decide.

### Output and internal columns

<a id="req-0205"></a>

**REQ-0205.** A column listed in `output.columns` is part of the artifact. Any
other declared column is internal: it is derived, converted, verified, and
shared with dependents exactly as an output column is, but omitted
from the artifact.

<a id="req-0210"></a>

**REQ-0210.** `output.columns` must not repeat a column or name an undeclared
column. Its entries select the artifact columns and control their order.

### Output identity

<a id="req-0220"></a>

**REQ-0220.** `keys` is an ordered list of columns named in
`output.columns` and must name at least one. A column must not be listed
twice. Validation fails for an empty list, unknown or internal column, or
repeated column, and reports the invalid name when present.

<a id="req-0221"></a>

**REQ-0221.** Once every column's lifecycle is complete, the combined key
values of each row must be non-missing and unique across the artifact. Key
validation happens before dataset verifications, which [Verification](../execution/verification.md) runs last.
String key values use [Text values](../values/text.md) equality. Key order is significant to [Lookup and joins](../operations/lookup.md),
which joins on the output keys a right side also carries. The applicable
keys are used for enrichment and do not change the identity asserted here.

### The artifact's path selects its profile

<a id="req-0715"></a>

**REQ-0715.** `output.path` is required and names the primary file; it has no
default. `output.warning_log` and
`output.verification_log` name [Verification](../execution/verification.md)'s
sidecars when the specification declares them.

<a id="req-0716"></a>

**REQ-0716.** The case-insensitive extension of `output.path` selects the
profile from the closed mapping below. An unmapped extension fails validation;
the run must not infer a profile from file contents or use a default.

| Extension | Profile | Container | What two runtimes must agree on |
|---|---|---|---|
| `.csv` | `csv` | delimited text | the bytes |
| `.parquet` | `parquet` | Parquet | schema, column/row order, values |

<a id="req-0717"></a>

**REQ-0717.** Retired; [REQ-0716](#req-0716) governs profile selection.

<a id="req-0718"></a>

**REQ-0718.** Retired; see [REQ-0716](#req-0716).

<a id="req-0719"></a>

**REQ-0719.** Retired; [REQ-0716](#req-0716) defines the two profiles.

<a id="req-0720"></a>

**REQ-0720.** A profile and the specification's `schema_version` identify the
bytes exactly. A consumer receives both. [Source ingestion](ingestion.md)'s producing-specification
link carries the whole producer document, not only the profile.

<a id="req-0721"></a>

**REQ-0721.** A profile's byte and value mapping is fixed by its producer's
`schema_version`. A mapping that diverges at the same schema version must use
a new profile name.

### Containers the mapping does not admit

<a id="req-1234"></a>

**REQ-1234.** The mapping is closed at two entries, and a container is added to
it only when it carries every value the language admits without changing one.
A profile must write [Text values](../values/text.md)'s
Unicode scalar sequences, [Numeric values](../values/numbers.md)'s binary64
floats and signed 64-bit integers, and [Temporal values](../values/temporal.md)'s
dates and local civil datetimes, under the declared names
[Specification structure](../specification/structure.md) admits. Truncation,
re-encoding, and rounding are prohibited.

<a id="req-1235"></a>

**REQ-1235.** SAS Transport v5 (`.xpt`) is not an output profile.

<a id="req-1237"></a>

**REQ-1237.** A derivation run publishes only the primary artifact and the
declared sidecars of [REQ-0193](#req-0193). Conversion to a transport container
is a separate packaging step outside this language.

<a id="req-1238"></a>

**REQ-1238.** Retired; [REQ-0716](#req-0716) and
[REQ-0760](#req-0760) govern unmapped extensions.

<a id="req-1233"></a>

**REQ-1233.** Dataset-JSON is generated by a study document under
[Dataset-JSON](../submission/dataset-json.md). `.json` is not a
specification output profile and fails under [REQ-0760](#req-0760).

### Publication

<a id="req-0752"></a>

**REQ-0752.** An artifact becomes visible in one step. An implementation:

1. writes the complete artifact into a temporary regular file in the same
   directory as the target;
2. flushes and closes that file, so its bytes reach the filesystem, not a
   buffer; and
3. atomically replaces the target with it.

<a id="req-0753"></a>

**REQ-0753.** The temporary file is regular and is in the target's own
directory. The replacement stays within one filesystem and remains
atomic. The name is not fixed and must not collide with the target or with
another run's temporary file.

<a id="req-0754"></a>

**REQ-0754.** A run that fails at any point leaves the target as it was and
removes its temporary file, so a failure produces neither an accepted artifact
nor residue. A reader observes either the artifact that was there before or the
complete new one, and never a prefix of the new one.

<a id="req-0755"></a>

**REQ-0755.** Publication happens once, after the whole artifact is complete:
after every value's lifecycle, key validation, and verification under [Execution lifecycle](../execution/lifecycle.md), and
after its rows are ordered. Rows are not streamed to the target as they are
constructed. A partially constructed dataset is not yet ordered, and a
run that fails midway would already have published part of it.

### Warning-log publication

<a id="req-0756"></a>

**REQ-0756.** `output.path` and `output.warning_log` must differ. Reusing one
path fails validation with `artifact_path_collision` and reports both fields.
Each path's extension independently selects its profile under [REQ-0716](publication.md#req-0716); the
primary may be Parquet while its log is CSV, or the reverse.

<a id="req-0757"></a>

**REQ-0757.** When a successful run has a warning log, a publisher renders
and validates both complete artifacts before touching either target. It
publishes the log before the primary artifact (after the verification log
when one is declared, per [REQ-1181](publication.md#req-1181)), applying [REQ-0752](publication.md#req-0752) through
[REQ-0754](publication.md#req-0754) to each file. A successful publication therefore never exposes a new
primary artifact without its completed log already visible.

<a id="req-0758"></a>

**REQ-0758.** Warning violations do not prevent publication. Failure to render
or replace the log is an output failure, not a warning: the primary target is
not touched. If replacing the primary fails after the log was replaced, the
publication fails and the prior primary remains; the complete log may remain as
the record of the completed candidate run. Atomic replacement is guaranteed per
file, not simultaneously across two paths.

### Verification-log publication

<a id="req-1180"></a>

**REQ-1180.** `output.verification_log` must differ from both
`output.path` and `output.warning_log`. Reusing a path fails validation with
`artifact_path_collision` and reports both fields. Its extension selects its
profile under [REQ-0716](publication.md#req-0716) independently of the other
two, and an extension the mapping does not name fails validation with
`unknown_artifact_profile`, the condition
[REQ-0760](publication.md#req-0760) raises for `output.path`.

<a id="req-1181"></a>

**REQ-1181.** The verification log [Verification](../execution/verification.md) fixes is diagnostic output,
not one of [REQ-0193](publication.md#req-0193)'s artifacts. A successful run
renders and validates it with the other files before touching any target and
publishes it first, then the warning log, then the primary artifact,
applying [REQ-0752](publication.md#req-0752) through
[REQ-0754](publication.md#req-0754) to each file. A failed run publishes
neither a primary artifact nor a warning log and replaces the verification log alone: it is the
only file a failed run writes, and writing it accepts nothing. Failure to
render or replace the verification log is an output failure that leaves every other
target untouched.

### Interface behavior

<a id="req-1047"></a>

**REQ-1047.** The `output_class` fields have these meanings:

| Field | Meaning |
| --- | --- |
| `output_class.path` | File this specification produces; [Artifact publication](publication.md) selects the serialization profile from its extension. |
| `output_class.decimals` | Fixed display digits for every float column of a csv artifact; [CSV profile](csv.md) defines the rounding and rejects it for parquet. |
| `output_class.columns` | Declared columns selected for the artifact, in the order this contract requires. |
| `output_class.warning_log` | Sidecar dataset recording warning-level verification violations; [Verification](../execution/verification.md) fixes its schema and [Artifact publication](publication.md) serializes it. |
| `output_class.verification_log` | Sidecar dataset recording the outcome of every declared verification; [Verification](../execution/verification.md) fixes its schema and [Artifact publication](publication.md) serializes it. |
| `output_class.order_by` | Terms ordering artifact rows after every verification; omission keeps [Execution lifecycle](../execution/lifecycle.md) construction order. |

## Error conditions

<a id="req-0233"></a>

**REQ-0233.** Retired; see [REQ-0220](#req-0220).

<a id="req-0234"></a>

**REQ-0234.** A missing `output.columns`, a duplicate entry, or an entry
naming an undeclared column: fail and report the column name.

<a id="req-0238"></a>

**REQ-0238.** A duplicate YAML mapping key, dataset identifier, column name,
or row ID: fail.

<a id="req-0239"></a>

**REQ-0239.** A conversion failure with no `missing` on the result wrapper
and without `strict: true`:
fail.

<a id="req-0240"></a>

**REQ-0240.** A missing or duplicate combined key value: fail and report the
offending rows. A specification without `rows` emits one row per key
combination under [REQ-0042](../execution/rows.md#req-0042), so a duplicate key can come only from row
templates emitting one combination more than once, under [REQ-0043](../execution/rows.md#req-0043).

<a id="req-0759"></a>

**REQ-0759.** A missing `output.path`: fail validation and report the
specification.

<a id="req-0760"></a>

**REQ-0760.** An `output.path` whose extension is outside the
mapping above, or none: fail validation with `unknown_artifact_profile`
and report the path. No extension is treated as a default.

<a id="req-0763"></a>

**REQ-0763.** A value that cannot be written under its column's
mapping: fail and report the column, the row's key, and the value.

<a id="req-0764"></a>

**REQ-0764.** A failed atomic replacement: fail and report the target. The run produces no
artifact, and the previous one is unchanged.
