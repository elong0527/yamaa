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
that order. Its rows are the rows [Row construction](../execution/rows.md) constructs. Every run also
builds [Verification](../execution/verification.md)'s run report. The report describes
the run and is neither a second derivation target nor a source within this
specification.

<a id="req-0195"></a>

**REQ-0195.** Serialization follows the [CSV](csv.md) and [Parquet](parquet.md) profiles. `output.path` and, when
present, `output.report` name the files this contract publishes, and each
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

Sponsor variables that may be missing can still define the presentation order
and a separate uniqueness check. Declare a non-missing row identity in `keys`,
the sponsor variables in `output.order_by`, and a dataset `unique` verification
over those variables. For example, when `AENDT` is missing for an ongoing
exposure, these fields accompany the declared exposure columns:

```yaml
keys: [USUBJID, EXSEQ]
output:
  path: adex.csv
  columns: [USUBJID, EXSEQ, EXTRT, ASTDT, AENDT]
  order_by: [USUBJID, EXTRT, ASTDT, AENDT]
verifications:
  - unique: {columns: [USUBJID, EXTRT, ASTDT, AENDT], id: sponsor-keys}
```

[Ordering](../execution/ordering.md#req-0299) places missing values last by
default. [REQ-0381](../execution/verification.md#req-0381) treats missing values
as values in the uniqueness check, so two rows with the same subject,
treatment, start date, and missing end date fail that check. The missing end
date remains missing. Submission key metadata describes the identity in
`keys`, as [REQ-0990](../submission/define-xml.md#req-0990) and
[REQ-1209](../submission/dataset-json.md#req-1209) require; neither ordering nor
the separate uniqueness check changes it.

### The artifact's path selects its profile

<a id="req-0715"></a>

**REQ-0715.** `output.path` is required and names the primary file; it has no
default. `output.report` names the file
[Verification](../execution/verification.md)'s run report is written to when
the specification declares it. `output.relative_to` selects the base for both
targets under [Resource resolution](resources.md#req-0780); `entry` lets a
shared output declaration publish separately for each study.

<a id="req-0716"></a>

**REQ-0716.** The case-insensitive extension of `output.path` selects the
profile from the closed mapping below, and `output.report` selects its own the
same way. An unmapped extension fails validation; the run must not infer a
profile from file contents or use a default.

| Extension | Profile | Container | What two runtimes must agree on |
|---|---|---|---|
| `.csv` | `csv` | delimited text | the bytes |
| `.parquet` | `parquet` | Parquet | schema, column/row order, values |

<a id="req-0719"></a>

**REQ-0719.** The `parquet` profile carries its own types: an artifact read
by another specification needs no declaration to be understood. The `csv`
profile's bytes are fixed exactly.

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
report file `output.report` declares under [REQ-0715](#req-0715). Conversion to
a transport container is a separate packaging step outside this language.

<a id="req-1233"></a>

**REQ-1233.** Dataset-JSON is generated by a study document under
[Dataset-JSON](../submission/dataset-json.md). `.json` is not a
specification output profile and fails under [REQ-0760](#req-0760).

### Publication

<a id="req-0752"></a>

**REQ-0752.** An artifact becomes visible in one step. An implementation:

1. writes the complete artifact into a temporary regular file in the target's
   directory or a private staging directory beneath it;
2. flushes and closes that file, so its bytes reach the filesystem, not a
   buffer; and
3. atomically replaces the target with it.

<a id="req-0753"></a>

**REQ-0753.** The temporary file is regular and stays in the target's own
filesystem. It is in the target's directory or a private staging directory
beneath it. A private staging directory belongs to the publishing user and
grants no access to other users. The publisher retains its directory descriptor
so replacement of its entry in the target's parent cannot substitute the checked
candidate. Temporary names or their private namespaces do not collide with the
target or another run's candidate. The final replacement remains atomic.

Native publishers coordinate writes through an exclusive selected-parent
directory lock. A lock refusal leaves the target untouched. An explicit save
intentionally replaces the current target; it is not a conditional update against
the target that existed when the publisher was created. The last successful
replacement supplies the visible artifact.

<a id="req-0754"></a>

**REQ-0754.** A run that fails at any point leaves the target as it was and
removes its temporary file, so a failure produces neither an accepted artifact
nor residue. A reader observes either the artifact that was there before or the
complete new one, and never a prefix of the new one.

If external interference or a filesystem failure prevents temporary cleanup,
the failure preserves the original operation error and reports the cleanup
failure and staging location. It does not claim that no residue remains.
Cleanup does not remove an entry whose ownership is unconfirmed or whose
observed identity has changed. The failed publication still produces no accepted
artifact and leaves the target untouched.

<a id="req-0755"></a>

**REQ-0755.** Publication happens once, after the whole artifact is complete:
after every value's lifecycle, key validation, and verification under [Execution lifecycle](../execution/lifecycle.md), and
after its rows are ordered. Rows are not streamed to the target as they are
constructed.

### Report publication

<a id="req-0756"></a>

**REQ-0756.** `output.path` and `output.report` must name different files,
and in a producer graph no node's `output.report` may name the file of another
node's `output.path` or `output.report` ([REQ-1330](publication.md#req-1330)).
In a project run, no two of these paths of the specifications in its run scope
may name one file ([REQ-1346](../specification/project.md#req-1346)).
Two of these paths that name one file, including two spellings that resolve to
the same file, fail validation with `artifact_path_collision` and report both
fields. The comparison uses the resolved locations without requiring either
file or its parent to exist.

<a id="req-1180"></a>

**REQ-1180.** Each path's extension selects its profile under
[REQ-0716](publication.md#req-0716) independently of the other: the primary may
be Parquet while the report is CSV, or the reverse. A report extension the
mapping does not name fails validation with `unknown_artifact_profile`, the
condition [REQ-0760](publication.md#req-0760) raises for `output.path`.

<a id="req-0757"></a>

**REQ-0757.** A successful run renders and validates every file it publishes
before touching any target. It publishes the declared report file first, then
the primary artifact, applying [REQ-0752](publication.md#req-0752) through
[REQ-0754](publication.md#req-0754) to each file. A successful publication
therefore never exposes a new primary artifact without its completed report
already visible. A run that declares no report file publishes only its primary
artifact.

<a id="req-1181"></a>

**REQ-1181.** A failed run publishes no primary artifact and replaces only its
declared report file, the diagnostic output of
[REQ-1177](../execution/verification.md#req-1177): it is the only file a failed
run writes, and writing it accepts nothing.

<a id="req-0758"></a>

**REQ-0758.** Warning violations do not prevent publication. A report that
cannot be rendered has already refused the build under
[REQ-0396](../execution/verification.md#req-0396), before publication. Failure
to replace the report file is an output failure that leaves every other target
untouched. If replacing the primary artifact fails after the report was
replaced, the publication fails and the prior primary artifact remains; the
complete report may remain as the record of the completed candidate run.
Atomic replacement is guaranteed per file, not simultaneously across paths.

### Producer-graph publication

<a id="req-1330"></a>

**REQ-1330.** An entry specification and every producer its `schema` links
reach under [REQ-0521](ingestion.md#req-0521) run as one producer graph, in
node order: every producer before each specification that reads it, and the
entry specification last. Each node is its own run and builds its own run
report. A node that fails stops the graph: no later node runs, and a node that
never ran builds no report. The host result exposes the entry specification's
report. [REQ-0756](publication.md#req-0756) compares the report paths of every
node, and an input that reads a producer's held artifact records that
artifact's byte length under [REQ-1176](../execution/verification.md#req-1176).
A project run instead builds each specification of its run scope once, with
the order, failure and publication rules of
[REQ-1341](../specification/project.md#req-1341) through
[REQ-1346](../specification/project.md#req-1346): there a failed specification
stops only the specifications that depend on it.

Save validates every target of the graph before touching any of them, then
publishes node by node in node order: a node that declares `output.report`
publishes its report immediately before its own primary artifact, as
[REQ-0757](publication.md#req-0757) orders the files of one specification. A
failed graph publishes no primary artifact; it replaces, in node order, the
report file of every node that ran and declares one, the failed node's
included. A failure while replacing one file stops the publication there: the
files already replaced remain, and no later target is touched.

### Interface behavior

<a id="req-1047"></a>

**REQ-1047.** The `output_class` fields have these meanings:

| Field | Meaning |
| --- | --- |
| `output_class.path` | File this specification produces; [Artifact publication](publication.md) selects the serialization profile from its extension. |
| `output_class.decimals` | Fixed display digits for every float column of a csv artifact; [CSV profile](csv.md) defines the rounding and rejects it for parquet. |
| `output_class.columns` | Declared columns selected for the artifact, in the order this contract requires. |
| `output_class.report` | File the run report is written to; [Verification](../execution/verification.md) fixes the report's schema and requires the file when any verification is a warning, and [Artifact publication](publication.md) selects its profile and publishes it. |
| `output_class.order_by` | Terms ordering artifact rows after every verification; omission keeps [Execution lifecycle](../execution/lifecycle.md) construction order. |

## Error conditions

<a id="req-0234"></a>

**REQ-0234.** A missing `output.columns`, a duplicate entry, or an entry
naming an undeclared column: fail and report the column name.

<a id="req-0238"></a>

**REQ-0238.** A duplicate YAML mapping key, dataset identifier, column name,
or row ID: fail.

<a id="req-0239"></a>

**REQ-0239.** A conversion failure with no `unconvertible` on the result
wrapper: fail.

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
