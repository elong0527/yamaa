# yamaa for Python

The `yamaa` distribution contains the Python facade and shared Rust engine.
Its public API consists of `domain`, `check`, `define`, `resolve_specification`, their result/error APIs,
the `Diagnostic` and `SourceSpan` record types, package version metadata, host
resource resolvers and the two ODM file helpers.

```python
import yamaa

built = yamaa.domain("spec.yaml", environment="python/environment.yaml")
built.output
built.issues
built.diagnostics
built.verification_log
built.warning_log
built.save()

checked = yamaa.check("spec.yaml", environment="python/environment.yaml")
checked.issues
checked.diagnostics
```

`environment` is optional. Pass a declared host environment explicitly when the
study uses project functions or terminology. Paths resolve relative to their
original declaring YAML layer by default. Set `relative_to: entry` on an input
dataset or `output` declaration to select the study entry's directory under the
[local path policy](../rules/reference/files.md). An input setting applies to
both its data path and producer `schema` link; output settings cover all logs
and the primary artifact.
There is no upward project-root discovery.

An input's `schema` link names the producing specification whose output columns
and declared types type a stored artifact. The link is metadata only: the
consumer never runs its producer. Build and save the producer first; the
consumer then reads the artifact at its own `path` like any other input, and
refuses a stored header or Parquet schema that differs from the producer's
output columns.

```python
yamaa.domain("dm.yaml").save()
built = yamaa.domain("suppdm.yaml")
```

## Host resources

Pass `resolver=` to `domain` or `check` to supply metadata, study inputs and
Save destinations from a host backend. Default calls keep the native local-file
resolver. A custom callback can read local files or retrieve remote objects;
it returns an explicit immutable reference plus bytes or an Arrow-compatible
frame. Authored paths keep their local spelling. A returned URI identifies the
host's object and is never interpreted as a filesystem path by the engine.

```python
from pathlib import Path
from yamaa.resources import MemoryResolver, SyntheticRemoteResolver

resources = MemoryResolver({
    "spec.yaml": Path("spec.yaml").read_bytes(),
    "raw.csv": b"ID\n7\n",
})
# Alternatively: resources.put_frame("raw.csv", immutable_arrow_or_polars_frame)
built = yamaa.domain("spec.yaml", resolver=resources)
built.resource_provenance
built.save()
resources.outputs  # exact held artifact bytes, keyed by their target reference
```

`SyntheticRemoteResolver` uses versioned `object://synthetic/` references with
the same deterministic behavior and needs no service credentials. Replace an
object with `put` or `put_frame`; a new build accepts its new version. Frames
use Arrow C Stream ownership without IPC or temporary input files. Supported
physical fields are UTF-8, Int64, Float64, Date32 and timestamps in whole seconds
without a timezone. Unsupported narrowing, timezone removal and fractional-second
truncation require an explicit preserving conversion by the host.

For another backend, wrap a callback in `ResourceResolver`. It receives a frozen
`ResourceRequest` and returns `ResourceResult` or raises a classified
`ResourceFailure`. Logical declaring locations begin at `/`; later requests
retain each metadata document's own location, including a linked producing
specification's. The operation contract is:

| Operation | Result |
| --- | --- |
| `inspect` | Object reference, without reading study data. |
| `capture` | Reference plus immutable bytes or a frame. |
| `target` | Output reference, without publishing. |
| `stage` | Private provisional token for the held output bytes. |
| `commit` | Atomically publish that token; return the held target reference. |
| `abort` | Remove the provisional token after failure or interruption. |

Abort is idempotent and must also accept a target reference when Stage raises
before returning a token. Save preflights every target and publishes logs before
primary output. A later failure keeps the completed
prefix and cleans the current provisional object. Source resources are never
reopened at Save. Original callback exceptions and interrupts propagate after
cleanup, including errors raised while exporting a frame.

Snapshots belong to one attempt. Reusing a logical request or reference compares
the original reference and held bytes or exact frame values, including float bits
and temporal precision. A changed identity or content is refused. Provenance
retains the first capture's logical location, authored spelling, reference and
representation; frames have typed evidence rather than fabricated empty source
bytes. Credentials stay inside the backend callback. There are no content hashes
or digest fields: explicit references and held-content comparison are the policy.

Project functions need their installed callable and declared conformance tests.
The environment's packaging `lock` is optional. Declare it to verify installed
called-package versions before activation; omit it to use the installed packages
without a version audit. A declared lock must exist and be valid.

A shared parent can use each study's own inputs and save in its own output folder:

```yaml
input:
  DM: {path: input/dm.csv, relative_to: entry}
output:
  path: output/adsl.csv
  relative_to: entry
  columns: [USUBJID]
```

Each study's entry inherits that parent. `save()` publishes its held bytes at
the selected study destinations without rerunning the build. A producing
specification linked through `schema` is read from its own file, which may also
inherit shared code; build and save it as its own entry before the consumer.

Inputs can use CSV, Parquet, SAS datasets (`.sas7bdat`), or SAS Transport v5/v8
(`.xpt`). SAS inputs explicitly declare `encoding`, for example
`{path: input/dm.xpt, encoding: windows-1252, types: {SEQ: int, VISITDT: date}}`.
Character fields default to `str`, numeric fields to `float`; numeric `types`
narrowing requires exact integral values. See the
[SAS source contract](../rules/reference/sas.md) for missing values, character
padding, supported encodings and 1960 date/datetime epochs. Outputs remain CSV
or Parquet.

Build reads inputs and invokes the declared project functions through the shared
engine. Check performs preparation/static validation without reading study data
or invoking project code. Each result owns its native state; owned Polars output,
issue and log views remain independent.

## Diagnostics

`diagnostics` is a list of frozen `yamaa.Diagnostic` records, one per finding
in the engine's authored order, and `issues` is the five-column Polars
projection of those same records: `phase`, `condition`, `requirement`,
`spec_paths` and `context` as the engine's canonical JSON text. Both views are
built from one native record, so they cannot disagree, and neither parses JSON
text in Python. `domain`, `check` and `define` results, `DefineError` after a
failed save and `ResolvedSpecificationError` all carry both.

```python
for finding in built.diagnostics:
    finding.phase, finding.condition, finding.requirement  # str, str, str | None
    finding.spec_paths                                     # list[str]
    finding.context                                        # dict[str, object]
    finding.source_span                                    # yamaa.SourceSpan | None
    finding.operand_route                                  # list[str] | None
```

`context` is structured: integer tokens are `int` with their exact value
whatever their width, other numbers `float`, objects `dict`, arrays `list`,
text `str` and null `None`. `source_span` is `SourceSpan(start, end)` with
half-open byte offsets into the original expression, and `operand_route` lists
the operand steps from the expression root to the failing node; both are
`None` when the failure site recorded no geometry. Absent geometry is
distinct from recorded zero geometry: `SourceSpan(0, 0)` is a zero-width span
and `[]` is a failure at the root. The public issue table has no `severity`
field. Signed integers retain their full 64-bit range; strings
retain Unicode and embedded NUL, and date/datetime columns retain ordinary Polars
types. Capacity refusals retain the shared condition and authored paths;
[release guidance](../docs/release.md#resource-refusals) describes their context.

`built.output_arrow`, `built.verification_log_arrow` and `built.warning_log_arrow`
return PyArrow tables through the Arrow C Stream/PyCapsule protocol, supported
with PyArrow 23.0.1 or newer. Every access creates an independent cursor and shares
the retained immutable value and validity buffers. A table remains valid after
the build handle, exporter or stream is released. The ordinary Polars views use
this same handoff without IPC serialization; Polars may copy string buffers and
convert whole-second timestamps to its supported unit. The Arrow views retain
the exact UTF-8, signed i64, null and epoch values, including NUL strings.
Requested schema conversion is refused; perform an explicit preserving Arrow
conversion after importing. See the [bridge ownership contract](../rust/README.md#arrow-host-handoff).

Numeric operations use PortableLibmV1 with pinned libm 0.2.16;
the [numeric implementation](../rust/crates/yamaa-core/src/numeric.rs) defines
the exact operations and domain checks.

Build publishes nothing. Explicit `save()` uses the declared destinations and
held bytes, without rereading inputs or rerunning callbacks. A failed build has
no accepted output and Save raises `yamaa.DomainError`; a diagnostic verification
sidecar can still be publishable. An operational publication failure returns
`False` and remains in issues. Publication preflights targets, then writes the
ordered log/output set. Replacement is atomic for each file; completed
writes remain if a later target fails. There is no whole-run file rollback.

## Resolved specification inspection

```python
artifact = yamaa.resolve_specification("study/spec.yaml")
facts = artifact.to_dict()
serialized = artifact.to_json()
restored = yamaa.ResolvedSpecification.from_json(serialized)
```

The [resolved artifact format](../rust/README.md#resolved-specification-artifact)
records the complete composed normalized model, field origins and each resource's
declaring source, selected base, authored spelling and location. A linked
producing specification is a `producer_schema` resource of its consumer, not a
separate entry. Resolution uses the same metadata preparation as direct
compilation and accepts the same optional `environment`; it invokes no callbacks.
Missing study data and nonexistent output directories can be inspected. Missing
required metadata still raises `ResolvedSpecificationError` with `issues` and
`diagnostics`.

Serialization preserves exact integer values and mapping order in a separate ledger.
Dictionary views are independent copies. Deserialization validates the distinct
format and version and replays inspection facts only; it does not compile a program,
open resources or rebind paths. Held facts remain available after source deletion or
a working-directory change. Use original source paths with `domain` and `check`.

## Implementation limits

The shared `domain`, `check` and `define` engine calls use the same
[resource policies](../docs/release.md#resource-limits) in Python and R.
That page owns the engine's capacity limits and refusal details. These are
implementation policies, not language constraints or process-memory guarantees.

## Define documents

```python
import yamaa

created = yamaa.define("environment.yaml", "sdtm", created="2026-09-11T00:00:00")
created.issues
created.save()
```

Define reads the selected environment submission section and its metadata and
parent specifications. A section's `transport: xpt` or
`transport: dataset_json` reads the already published CSV/Parquet artifacts and
writes one SAS Transport version 5 or Dataset-JSON file per dataset beside
`define.xml`. It executes no specification, reads no ordinary study source and
opens no supporting-document links. The shared lifecycle renders/validates the
whole output set before Save, then publishes the transport files in section
order and XML last. A failed Save raises `yamaa.DefineError` carrying `issues`
and `diagnostics`. See the
[submission contracts](../rules/README.md#rule-files).

## ODM file helpers

```python
from yamaa import read_odm, write_odm_parquet

frame = read_odm("input.xml")
written = write_odm_parquet("input.xml", "clinical-items.parquet")
print(written.output_path, written.row_count)
```

These helpers read ODM 1.3/2.0 XML or TAR inputs into the fixed
clinical-item schema. They preserve source order and absent/null/empty values,
and the Parquet writer uses bounded batches and atomic publication. Their
`max_archive_members`, `max_expanded_bytes` and `max_xml_depth` options govern
parsing; parser refusals raise `ValueError`. Their
[private implementation](src/yamaa/_odm/README.md) imports only file/schema
libraries. Internal row
models, iterators, binding plans and contexts are not public authoring APIs.

## Specification style

The style CLI uses packaged field order and the shared native YAML/schema
services. Style findings are authoring advice, separate from validation diagnostics.

```bash
python -m yamaa.style study/adsl.yaml study/adae.yaml
python -m yamaa.style study/
python -m yamaa.style --fix study/adsl.yaml
python -m yamaa.style --ignore line_width study/
```

Findings print `path:line:column: name message`. Exit status is 0 when
no finding remains, 1 when findings remain and 2 when input cannot be read/parsed.
`--fix` keeps a layout edit only when the shared admission/equality proof accepts
it and comments remain unchanged. Authoring lints that require a semantic rewrite
offer no automatic fix. See the [style guide](../docs/articles/style.md).
Its Python implementation names remain private.

## Installation

Download the wheel for your supported OS from the selected
[GitHub release](https://github.com/elong0527/yamaa/releases), then run
`python -m pip install ./yamaa-0.4.0-<wheel-tags>.whl`. The standalone source
asset installs with `python -m pip install ./yamaa-0.4.0.tar.gz`; source builds
need Rust 1.90.0/Cargo, a C compiler and GNU make. Release wheels are the exact
qualified artifacts; the attached source archive is the tested Linux archive.
The other OS source lanes independently build and qualify their own archives.
GitHub distribution does not imply publication to PyPI.

For repository setup and verification, follow the
[contributor guide](../CONTRIBUTING.md). Supported interpreters, platforms and
complete installed qualification are defined in [release guidance](../docs/release.md).
