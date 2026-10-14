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

Build reads inputs and invokes the declared project functions through the shared
engine. Check performs preparation/static validation without reading study data
or invoking project code. Each result owns its native state; copied Polars output,
issue and log views remain independent. Issues contain `phase`, `condition`,
`requirement`, `spec_paths` and JSON-text `context`. The public issue table has no
`severity` field. Signed integers retain their full 64-bit range; strings
retain Unicode and embedded NUL, and date/datetime columns retain ordinary Polars
types. Numeric operations use PortableLibmV1 with pinned libm 0.2.16;
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

The shared engine admits up to 1,024 declared columns, output columns and fields
in each input's `types`. CSV, Parquet, Arrow imports and producer contracts use
the same width ceiling. An `ordinal` adds one field and counts toward this bound.
Specifications still admit at most 64 keys and 16 inputs.

Width does not override aggregate budgets: the default source allowance is
2,097,152 cells and output allowance is 1,048,576 cells. At 1,024 fields these
allow at most 2,048 source rows and 1,024 output rows, before other limits apply.
Validation and normalization share 67,108,864 logical work units; complex or
inherited specifications can exhaust this budget below the width ceiling.
These are implementation policies, not language constraints or process-memory
guarantees. Broader capacity and inheritance work remains tracked in
[#2185](https://github.com/elong0527/yamaa/issues/2185) and
[#2288](https://github.com/elong0527/yamaa/issues/2288).

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

Build the source distribution with Rust 1.90.0 and Cargo. For a locked
development installation from the repository root:

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
