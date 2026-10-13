# yamaa for Python

The migration candidate packages the Python facade and shared Rust engine in one
`yamaa` distribution. The approved release surface is `domain`, `check`, `define`,
their result/error APIs, package version metadata, and the two ODM file helpers.
Full installed qualification and the [transition gates](../rust/planning/transition.md)
remain required before release and reference removal.

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
legacy `severity` field. Signed integers retain their full 64-bit range; strings
retain Unicode and embedded NUL, and date/datetime columns retain ordinary Polars
types. The numerical policy is [PortableLibmV1](../rust/planning/1740-math-decision.md).

Build publishes nothing. Explicit `save()` uses the declared destinations and
held bytes, without rereading inputs or rerunning callbacks. A failed build has
no accepted output and Save raises `yamaa.DomainError`; a diagnostic verification
sidecar can still be publishable. An operational publication failure returns
`False` and remains in issues. Publication preflights targets, then writes the
ordered producer/log/output set. Replacement is atomic for each file; completed
writes remain if a later target fails. There is no whole-run file rollback.

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
[native Define contract](../rust/DEFINE_GENERATION.md).

## ODM file helpers

```python
from yamaa import read_odm, write_odm_parquet

frame = read_odm("input.xml")
written = write_odm_parquet("input.xml", "clinical-items.parquet")
print(written.output_path, written.row_count)
```

These retained helpers read ODM 1.3/2.0 XML or TAR inputs into the fixed
clinical-item schema. They preserve source order and absent/null/empty values,
and the Parquet writer uses bounded batches and atomic publication. Their
[private implementation](src/yamaa/_odm/README.md) imports only file/schema
libraries, with no reference specification/evaluator dependency. Internal row
models, iterators, binding plans and contexts are not public authoring APIs.

## Specification style

The retained style CLI uses packaged field order and the shared native YAML/schema
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

Use the qualified wheel for the supported platform/interpreter, or build the
source distribution with Rust 1.90.0 and Cargo. The planned shared release is
0.4.0; these migration branches have not published it. For a locked development
installation from the repository root:

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

## Migration from the reference API

| Previous usage | Approved replacement |
| --- | --- |
| `yamaa.yamaa_domain(...)`, `DomainRun` | `yamaa.domain(specification, environment=...)` and its owned result. |
| `DomainRunError` | `yamaa.DomainError`. |
| `save(path)` | Declare the destination in YAML, then call `save()` without arguments. |
| `.spec`, `.inputs`, `.input`, resolved-spec/provenance helpers | These authoring views have no retained public producer. Use authored specifications and the public output/issues/logs. |
| `specification.load_specification` | `yamaa.check(...)` for diagnostic validation; strict Python model objects are not retained. |
| `generate_study_document(define.yaml)` | `yamaa.define(environment, standard, created=...)` using the environment submission section. Update the authored metadata to that contract. |
| Project/schema/data root overrides and `yamaa-project.yaml` discovery | Declaring-file paths and an explicitly passed environment. |
| `yamaa.odm.read_odm`, `write_odm_parquet` | Import both helpers from `yamaa`; their implementation is private. |
| Public expression/planning/runtime/verification/IO/schema/model/ODM-binding extensions | Retired under the approved first-release scope. Domain/check/Define use the shared engine directly. |

Reference code and measurement artifacts remain during qualification. Removal
follows [the family inventory](../rust/planning/python-cutover.md), after the
complete matrix, native validator, performance/rollback and transition approval
pass. Pin one whole prior release in a fresh environment before starting a
rollback replay; do not switch an outstanding build or retained Save to another
backend. Previously saved outputs retain their original release provenance.
