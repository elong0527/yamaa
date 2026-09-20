# Project functions

A specification names a logical function; a project supplies the code. This
module is the boundary between the two. R018 keeps the language and the
callable out of the specification entirely, so the same `spec.yaml` runs in
an R project and in a Python one, and the runner -- not the specification --
chooses which project root answers.

```python
from yamaa.functions import execute_with_project_functions
from yamaa.io import ProjectResources, load_source_tables
from yamaa.specification import load_specification

specification = load_specification("benchmark/adam-adsl-bmi-function/spec.yaml", "yaml")
result = execute_with_project_functions(
    specification.specification,
    lambda datasets: load_source_tables(datasets, ProjectResources(example_directory)),
    "python/tests/projects/bmi-python",  # the selected project root
    "yaml",  # the schema bundle
)
```

The root is a positional argument because REQ-0663 makes it one: it comes
from the runner, and a specification can neither name it nor override what
it says. Executing the same specification with no root selected reports
`function` as an unimplemented operation rather than inventing a result,
which is the portable case REQ-0662 describes.

## The order a run happens in

1. **Resolve** exactly `environment.yaml` at the selected root and validate
   it against `schema_environment.yaml` on its own (`environment.py`).
   Contracts, signatures, bindings, and vector documents are all settled
   here, before anything is loaded.
2. **Check the calls** the specification writes against the contracts this
   project actually provides (`calls.py`): the logical name, the exact
   contract version, and a closed, exactly typed argument list.
3. **Verify the artifact** (`artifact.py`). REQ-0667 puts the runner language
   and the digest before activation, so a project pinned to bytes that are
   not there never gets as far as importing anything.
4. **Activate** (`activation.py`): resolve every binding inside the verified
   artifact, then run every vector. REQ-0691 puts all of them before any
   specification executes, so a contract whose implementation has drifted
   fails against its own vectors rather than against study data.
5. **Execute**, on a dispatcher carrying one extra operation
   (`evaluator.py`). A call then sits wherever an expression sits, and
   `invocation.py` crosses the host boundary once per logical row.

A failure at any of these stages is an ordinary execution failure carrying
the condition R018 names, reported against the specification text that
required project code. The source provider is never called before step 4
finishes.

## Artifacts and the local resolver

REQ-0666 pins the runtime by SHA-256 content identity and makes that identity
the only place a binding is resolved: the process search path, the working
directory, and an ambient installation answer for nothing. A binding is
imported into a private package whose name carries the digest, with the
artifact directory as its only search path, so `projectbmi.bmi` reaches the
artifact's module and a same-named installed package is invisible to it.

An organization resolver maps `runtime.artifact.reference` to wherever that
organization publishes runtimes. `ProjectArtifactDirectory` is the local
one: a project root that vendors its runtime at `runtime/` serves it from
there, and the declared digest still decides whether those bytes are the
artifact that was pinned. `MappedArtifacts` takes an explicit mapping when
a runner resolved references itself.

The digest of a directory is a hash over a manifest of every file it
carries -- each file's path relative to the artifact root, then the hash of
its bytes, in path order. A host bytecode cache (`__pycache__`, `.pyc`) is
excluded: it is written beside the sources it was compiled from, is not
project code, and would otherwise change an artifact's identity merely by
running it.

## What crosses the boundary

- **Arguments.** REQ-0681 is applied before the host sees anything: an
  omitted optional argument selects its environment default, and a missing
  value for a non-accepting parameter short-circuits, so the binding is not
  invoked and the result is missing. REQ-0682 makes that missing result one
  the contract did not have to declare. A missing value for an accepting
  parameter arrives as `None`.
- **Types.** REQ-0678 admits no conversion, `int` to `float` included. A
  variable argument is checked against its declared type at the
  implementation stage and again against the runtime value it carries. A
  `date` or `datetime` reaches the binding as `datetime.date` or a
  zone-free `datetime.datetime`.
- **Results.** R011's non-finite normalization runs immediately after the
  host returns, so an infinity is a missing result that a contract must
  have declared. A returned value of another type, another shape, a
  Boolean, a zoned or sub-second datetime, or an undeclared missing is
  `invalid_function_result` and is not converted; a host exception is
  `function_call_failed`. REQ-0701 and REQ-0702 are both fatal and neither
  admits an R008 local handler.

## Comparing results

REQ-0692 compares a float result with its expected value through temporary
decimal copies at the contract's `comparison_decimals`, with an exact
decimal tie going away from zero. That is the same rounding REQ-0747 fixes
for display, so `yamaa.io.csv.fixed_point` performs both and the two cannot
disagree. REQ-0693 keeps the comparison off the value: a run holds the
unrounded result, and rounding for display happens once, later, under
`output.decimals`.

## Where the boundary of this module is

- **The engine never imports this module.** `yamaa_domain` accepts an
  optional `dispatcher` -- a generic expression-dispatch hook the engine
  threads through unchanged -- and the runner in this module builds it from
  the activated project. Benchmarks use the runner the way they use the
  engine, with no internal machinery in `run.py`:

  ```python
  import yamaa
  from yamaa.functions import run_with_project_functions

  adsl = run_with_project_functions("spec.yaml", project_root="python").output
  ```

  All project-function orchestration (environment, calls, artifact,
  activation) lives here, outside the engine; the engine only ever sees a
  dispatcher.
- **Static validation owns the coverage obligations.** REQ-0689 has the
  static validator check that each contract's vectors demonstrate `normal`,
  `boundary`, every default, every missing behavior, both values of every
  Boolean parameter, `nullable-output`, and `numeric-comparison`. This
  module validates that a vector document is structurally sound and
  identifies its own contract, and then runs every case in it.
- **The fingerprint is shared.** REQ-0671 is implemented here and in
  `.github/scripts/yaml-validation/validate_repository.py`, and the tests
  check the two produce the same bytes for the same contract. A contract
  claimed in two projects is one contract only when they do.
- **Nothing here reads an artifact from `expected/`.** The committed
  goldens are read by tests.

## A Python project root

`python/tests/projects/bmi-python` is a complete one, and implements the
same logical contract the committed `adam-adsl-bmi-function` example
implements in R:

```text
bmi-python/
  environment.yaml        # language: python, artifact reference and digest
  conformance/bmi.yaml    # the same vector content the R root runs
  runtime/projectbmi.py   # the pinned code the binding resolves to
```

The two roots calculate one contract fingerprint and run byte-identical
vectors, which is what REQ-0675 and REQ-0690 require of two projects claiming
one contract. A contract that needs no renaming writes its `environment.yaml`
without the two defaulted declarations: an omitted `binding.args` maps each
logical parameter to the same-named host argument, and an omitted
`implementation_version` is the environment `version`. When several roots
implement one contract, each entry names the shared document once in
`contract` (a project-root-local `contracts.yaml` holding `contract_version`,
`description`, `params`, and `returns` per function) instead of repeating
those fields inline; the entry keeps its own binding and conformance path.
Re-pinning after changing the code is one call:

```python
from yamaa.functions import artifact_digest

print(artifact_digest(Path("python/tests/projects/bmi-python/runtime")))
```

## Focused tests

```bash
uv run --project python --no-sync pytest python/tests/functions
```
