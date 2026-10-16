# yamaa for Python

The `yamaa` distribution contains the Python facade and shared Rust engine.
Its public API consists of `domain`, `check`, `define`, `resolve_specification`, their result/error APIs,
package version metadata, and the two ODM file helpers.

```python
import yamaa

built = yamaa.domain("spec.yaml", environment="python/environment.yaml")
built.output
built.issues
built.verification_log
built.warning_log
built.save()

checked = yamaa.check("spec.yaml", environment="python/environment.yaml")
checked.issues
```

`environment` is optional. Pass a declared host environment explicitly when the
study uses project functions or terminology. Paths resolve relative to their
original declaring YAML layer by default. Set `relative_to: entry` on an input
dataset or `output` declaration to select the study entry's directory under the
[local path policy](../rules/storage/resources.md). An input setting applies to
both its data path and producer `schema` link; output settings cover all logs
and the primary artifact.
There is no upward project-root discovery or public backend selector in this API.

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
the selected study destinations without rerunning the build. A producer linked
through `schema` uses its own entry file, which may also inherit shared code.

Inputs can use CSV, Parquet, SAS datasets (`.sas7bdat`), or SAS Transport v5/v8
(`.xpt`). SAS inputs explicitly declare `encoding`, for example
`{path: input/dm.xpt, encoding: windows-1252, types: {SEQ: int, VISITDT: date}}`.
Character fields default to `str`, numeric fields to `float`; numeric `types`
narrowing requires exact integral values. See the
[SAS source contract](../rules/storage/sas.md) for missing values, character
padding, supported encodings and 1960 date/datetime epochs. Outputs remain CSV
or Parquet.

Build reads inputs and invokes the declared project functions through the shared
engine. Check performs preparation/static validation without reading study data
or invoking project code. Each result owns its native state; copied Polars output,
issue and log views remain independent. Issues contain `phase`, `condition`,
`requirement`, `spec_paths` and JSON-text `context`. The public issue table has no
`severity` field. Signed integers retain their full 64-bit range; strings
retain Unicode and embedded NUL, and date/datetime columns retain ordinary Polars
types. Capacity refusals retain the shared condition and authored paths;
[release guidance](../docs/release.md#resource-refusals) describes their context.

Numeric operations use PortableLibmV1 with pinned libm 0.2.16;
the [numeric implementation](../rust/crates/yamaa-core/src/numeric.rs) defines
the exact operations and domain checks.

Build publishes nothing. Explicit `save()` uses the declared destinations and
held bytes, without rereading inputs or rerunning callbacks. A failed build has
no accepted output and Save raises `yamaa.DomainError`; a diagnostic verification
sidecar can still be publishable. An operational publication failure returns
`False` and remains in issues. Publication preflights targets, then writes the
ordered producer/log/output set. Replacement is atomic for each file; completed
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
declaring source, selected base, authored spelling and location. Linked producers
have their own entries. Resolution uses the same metadata preparation as direct
compilation and accepts the same optional `environment`; it invokes no callbacks.
Missing study data and nonexistent output directories can be inspected. Missing
required metadata still raises `ResolvedSpecificationError` with structured `issues`.

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
parent specifications. Dataset-JSON reads already published CSV/Parquet artifacts
when requested. It executes no specification, reads no ordinary study source and
opens no supporting-document links. The shared lifecycle renders/validates the
whole output set before Save, then publishes Dataset-JSON in section order and
XML last. A failed Save raises `yamaa.DefineError` carrying issues. See the
[submission contracts](../rules/README.md#submission).

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
parsing; parser refusals raise `ODMError`. Their
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

Findings print `path:line:column: name message (REQ-NNNN)`. Exit status is 0 when
no finding remains, 1 when findings remain and 2 when input cannot be read/parsed.
`--fix` keeps a layout edit only when the shared admission/equality proof accepts
it and comments remain unchanged. Authoring lints that require a semantic rewrite
offer no automatic fix. See the [style contract](../rules/specification/style.md).
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
