# Optional native specification prototype

`native_datasets.execute_with_source_provider` accepts a `Specification` produced
by the existing schema loader and an explicit source provider. It returns a
`NativeDatasetRun` containing the ordinary `ExecutionSuccess`, `ExecutionFailure`
or `ExecutionUnsupported` result and all completed dataset `VerificationRecord`s.
It does not change `Domain`, CLI or conformance backend selection. Python remains
the default and the native installation flag remains `execution_supported=false`.

```python
from pathlib import Path
from yamaa.adapters.native_datasets import execute_with_source_provider
from yamaa.io import ProjectResources, load_source_tables, render_artifact
from yamaa.specification import load_specification

case = Path("benchmarks/adam-adlb-ordered-sum")
specification = load_specification(case / "spec.yaml", Path("yaml")).specification
resources = ProjectResources(case)
run = execute_with_source_provider(
    specification,
    lambda declarations: load_source_tables(declarations, resources),
)
if run.result.status == "success":
    csv_bytes = render_artifact(run.result.artifact)
```

Install the `yamaa` package, which includes the private `yamaa._native` extension.
The example renders bytes in memory; this API never publishes files. A caller
supplies approved source access through the existing `ProjectResources` port.
Arbitrary hand-constructed models that bypass schema/YAML validation are outside
this interface's input contract.

## Ownership and admitted semantics

The frontend rejects unimplemented syntax across the whole normalized specification
before requesting sources. It checks aggregate grammar rather than mistaking
malformed syntax for a valid unimplemented expression. The native entrypoint must
exist before the provider runs. Row-filter, assert and key-grain requests also require the native
`dataset_capabilities()` advertisement before provider effects. Missing or
incompatible requested capability returns `ExecutionUnsupported`. Source-independent
filter scope/phase errors are also checked before IO, including grouped source
references and unavailable row columns; valid row-local defaults remain available
under REQ-1260. Binding against actual source schemas follows
source ingestion; Rust admits the complete bound request before IPC decoding.
The admitted specification is copied before provider effects, and the provider
receives separate source declarations so nested mutable model data cannot replace
the plan during IO. There is one provider invocation and one native dataset invocation, with no fallback,
reference evaluation or reference verification. This prototype refuses project
function expressions; public project environments use the shared file pipeline.

## Public project environments

The superseded `execute_with_project_functions` entrypoint, artifact resolver and
activation cache are retired by #1757. Use `yamaa.check(spec_path,
environment=environment_path)` for static admission and `yamaa.domain(spec_path,
environment=environment_path)` for a fresh shared build. Function definitions
name normally installed package code; the locked host verifies called versions
and runs all called-function tests before any study read, on every build.
See [the public package documentation](../../../README.md) and
[the qualification record](../../../../rust/planning/1757-qualification.md).

Private typed callback protocols remain independently qualified component seams.
They do not replace the public environment lifecycle or establish whole producer
workflow qualification (#1741).
