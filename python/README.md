# yamaa for Python

The `yamaa` distribution contains the Python facade and shared Rust engine.
Its public API consists of `domain`, `check`, `define`, their result/error APIs,
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
original declaring YAML layer under the [local path policy](../rules/storage/resources.md).
There is no upward project-root discovery or public backend selector in this API.

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
types. A compiler capacity refusal keeps `engine_rejected` and a null
requirement. Its context names `code: compiler_limit`, `stage: prepare`,
`resource`, integer `limit` and `actual` when counted; `spec_paths` identifies
the refused declaration. The same facts are returned by check and build,
with or without an environment. Numeric policy refusals retain their applicable
resource bound under `code: numeric_policy`. Layered preparation and YAML
resource refusals use `code: resource_limit` and retain the preparation or
decoder `phase` alongside the resource bound in context.

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

## Implementation limits

Implementation limits live in one place: the
[release resource policies](../docs/release.md#resource-limits) state the
shared width ceilings, the longitudinal row/cell/text profile the public
hosts use, and the validation, normalization and representation budgets.
They are implementation policies, not language constraints or
process-memory guarantees.

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

## Install and verify

Download the wheel for your supported OS from the selected
[GitHub release](https://github.com/elong0527/yamaa/releases), then run
`python -m pip install ./yamaa-0.4.0-<wheel-tags>.whl`. The standalone source
asset installs with `python -m pip install ./yamaa-0.4.0.tar.gz`; source builds
need Rust 1.90.0/Cargo, a C compiler and GNU make. Release wheels are the exact
qualified artifacts; the attached source archive is the tested Linux archive.
The other OS source lanes independently build and qualify their own archives.
GitHub distribution does not imply publication to PyPI.

For a locked development installation of a checkout candidate from the
repository root:

```bash
uv sync --project python --extra test --locked --no-editable
uv run --project python --no-sync ruff format --check python/src/yamaa python/tests
uv run --project python --no-sync ruff check python/src/yamaa python/tests
```

The complete installed native workflow qualifies wheel/source packages outside
the checkout on CPython 3.12-3.14, Linux x86_64, macOS arm64 and Windows x64.
Public benchmark conformance compares independently committed expected data;
installed supplemental suites cover argument/result/ownership, callbacks,
producer order/logs, exact scalar values, grammar, Define and retained ODM I/O.
A local source import or component test alone does not establish release parity.
