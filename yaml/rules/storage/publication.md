---
id: storage/publication
title: Artifact publication
status: normative
---

# Artifact publication

## Purpose

Select artifact columns and profiles and publish complete files atomically.

## Scope and dependencies

This contract owns the requirements below. Related contracts:

- [Execution lifecycle](../execution/lifecycle.md).
- [Verification](../execution/verification.md).
- [Lookup and joins](../operations/lookup.md).
- [Specification structure](../specification/structure.md).
- [Source ingestion](ingestion.md).
- [Text values](../values/text.md).

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
also produce [Verification](../execution/verification.md)'s governed warning-violation sidecar. That log reports
the run, not a second derivation target or source within this specification.

<a id="req-0195"></a>

**REQ-0195.** Serialization follows the [CSV](csv.md) and [Parquet](parquet.md) profiles. `output.path` names the primary
file and `output.violation_log`, when present, names [Verification](../execution/verification.md)'s sidecar. Each
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
twice.

<a id="req-0221"></a>

**REQ-0221.** Once every column's lifecycle is complete, the combined key
values of each row must be non-missing and unique across the artifact. Key
validation happens before dataset verifications, which [Verification](../execution/verification.md) runs last.
String key values use [Text values](../values/text.md) equality. Key order is significant to [Lookup and joins](../operations/lookup.md),
which joins on the output keys a right side also carries. The applicable
keys are used for enrichment and do not change the identity asserted here.

### The artifact's path selects its profile

<a id="req-0715"></a>

**REQ-0715.** `output.path` names the primary file the specification produces. It
is required: a specification that derives an artifact says what it produces,
and there is no default name for one. `output.violation_log` names [Verification](../execution/verification.md)'s
sidecar when the specification declares one.

<a id="req-0716"></a>

**REQ-0716.** The path's extension selects the profile. The mapping is closed, so
an extension outside it names no profile and fails validation rather than
falling back to one. The extension is matched without regard to case, because a
study that stores `ADSL.CSV` names the same container as one that stores
`adsl.csv` and two runtimes must not disagree about which.

| Extension | Profile | Container | What two runtimes must agree on |
|---|---|---|---|
| `.csv` | `csv` | delimited text | the bytes |
| `.parquet` | `parquet` | Parquet | schema, column/row order, values |

<a id="req-0717"></a>

**REQ-0717.** One field carries both facts: a specification that produces
an artifact needs a path regardless. A separate profile beside it could
disagree with the name it writes -- an `adsl.csv` declared `parquet` is a file
whose name lies about its contents. The cost is stated rather than hidden:
renaming the artifact changes the container, so a rename is a change to the
contract and not only to a filename.

<a id="req-0718"></a>

**REQ-0718.** Deriving is not guessing. The extension is read from the
specification, where a reviewer sees it, against a closed mapping this contract
fixes; an unrecognized extension stops the run. A reader that instead sniffed a
file's contents, or accepted an unknown extension under a default, could read a
conforming artifact wrongly without failing, and neither is permitted.

<a id="req-0719"></a>

**REQ-0719.** The two profiles exist for different readers. `parquet` is the
production container: it carries its own types, so an artifact read by another
specification needs no declaration to be understood, and a large one does not
pay for decimal text. `csv` is the reviewable container: a human can read it, a
diff can show what moved in it, and its bytes are fixed exactly, which is what
makes it usable as a golden contract.

<a id="req-0720"></a>

**REQ-0720.** A profile and the specification's `schema_version` identify the
bytes exactly. A consumer receives both because [Source ingestion](ingestion.md)'s producing-specification
link carries the whole producer document, not only the profile.

<a id="req-0721"></a>

**REQ-0721.** A later release that changes any byte-level or mapping decision
below therefore changes what a profile means at that schema version, and an
artifact keeps the meaning its producer's version gives it. A profile that ever
has to diverge from the schema version is added as a name rather than by
redefining one of these two.

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
directory so that the replacement stays within one filesystem and remains
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
constructed, because a partially constructed dataset is not yet ordered and a
run that fails midway would already have published part of it.

### Violation-log publication

<a id="req-0756"></a>

**REQ-0756.** `output.path` and `output.violation_log` must differ. Reusing one
path fails validation with `artifact_path_collision` and reports both fields.
Each path's extension independently selects its profile under [REQ-0716](publication.md#req-0716); the
primary may be Parquet while its log is CSV, or the reverse.

<a id="req-0757"></a>

**REQ-0757.** When a successful run has a violation log, a publisher renders
and validates both complete artifacts before touching either target. It
publishes the log first and the primary artifact last, applying [REQ-0752](publication.md#req-0752) through
[REQ-0754](publication.md#req-0754) to each file. A successful publication therefore never exposes a new
primary artifact without its completed log already visible.

<a id="req-0758"></a>

**REQ-0758.** Warning violations do not prevent publication. Failure to render
or replace the log is an output failure, not a warning: the primary target is
not touched. If replacing the primary fails after the log was replaced, the
publication fails and the prior primary remains; the complete log may remain as
the record of the completed candidate run. Atomic replacement is guaranteed per
file, not simultaneously across two paths.

### Interface behavior

<a id="req-1047"></a>

**REQ-1047.** The `output_class` interface has the following meanings. Shape, defaults, and
structural constraints come from its schema declaration.

| Field | Meaning |
| --- | --- |
| `output_class.path` | File this specification produces; [Artifact publication](publication.md) selects the serialization profile from its extension. |
| `output_class.decimals` | Fixed display digits for every float column of a csv artifact; [CSV profile](csv.md) defines the rounding and rejects it for parquet. |
| `output_class.columns` | Declared columns selected for the artifact, in the order this contract requires. |
| `output_class.violation_log` | Sidecar dataset recording warning-level verification violations; [Verification](../execution/verification.md) fixes its schema and [Artifact publication](publication.md) serializes it. |
| `output_class.order_by` | Terms ordering artifact rows after every verification; omission keeps [Execution lifecycle](../execution/lifecycle.md) construction order. |

## Error conditions

<a id="req-0233"></a>

**REQ-0233.** An internal column named in `keys`: fail and report the column
name.

<a id="req-0234"></a>

**REQ-0234.** A missing `output.columns`, a duplicate entry, or an entry
naming an undeclared column: fail and report the column name.

<a id="req-0238"></a>

**REQ-0238.** A duplicate YAML mapping key, dataset identifier, column name,
or row ID: fail.

<a id="req-0239"></a>

**REQ-0239.** A conversion failure with no `conversion_failure` handler:
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

## Conformance examples

Representative specifications, input data, and expected outcomes:

- [negative-keys-internal](../../../benchmark/negative-keys-internal/README.md).
- [negative-output-duplicate](../../../benchmark/negative-output-duplicate/README.md).

The [execution manifest](../../../benchmark/execution-manifest.yaml) records
which fixtures execute. Grammar contracts additionally replay their shared
vectors. Static validation does not establish runtime parity.

## Rationale

Select artifact columns and profiles and publish complete files atomically. Keeping this topic in one contract lets
other owners refer to it without defining a second policy.
