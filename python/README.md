# yamaa Python

The Python package derives one output dataset from one yamaa specification:
it validates the specification, builds and derives its rows, runs its
verifications, and returns the dataset. It needs only PyYAML and PyArrow.
The package supports Python 3.11 and newer; CI exercises Python 3.11 and 3.14.

## Install and test

From the repository root, create the locked test environment and run all Python
checks:

```bash
uv sync --project python --extra test --locked --no-editable
uv run --project python --no-sync ruff format --check python/src/yamaa python/tests
uv run --project python --no-sync ruff check python/src/yamaa python/tests
uv run --project python --no-sync pytest python/tests
```

For installation with `pip`:

```bash
python -m pip install './python[test]'
python -m pytest python/tests
```

Tests import the installed package. They do not require `PYTHONPATH` or depend
on the repository's current working directory.

## Derive a dataset

`yamaa.derive(spec_path, project_root=None)` runs one specification and
returns its output dataset as CSV text:

```python
import yamaa

dm = yamaa.derive("spec.yaml")
```

Input paths resolve against the specification's directory. `derive` writes
nothing to disk; `output.path` names the artifact the dataset describes, and
the caller decides where to keep the returned text.

A specification that calls project functions (`function:` derivations) names
the directory holding its `environment.yaml` as `project_root`:

```python
adsl = yamaa.derive("spec.yaml", project_root="python")
```

Only the Python runtime is supported; an environment whose
`runtime.language` names another language fails as `runner_language_mismatch`.

## Failures

Every validation, derivation, and verification failure raises
`yamaa.YamaaError`, which carries the same fields a negative benchmark pins in
its `expected/error.yaml`:

```python
try:
    yamaa.derive("spec.yaml")
except yamaa.YamaaError as error:
    error.phase        # "validation", "join", "derivation", "verification", ...
    error.condition    # a condition registered in yaml/conditions.yaml
    error.requirement  # the REQ-NNNN the failure cites
    error.spec_paths   # where in the specification it failed
    error.context      # the values the rule says to report
```

## Inputs

An input is a CSV or Parquet file. A Parquet field takes its type from the
Parquet schema; a CSV field is `str` unless `input.<name>.types` declares
otherwise. An input bound to the Operational Data Model (ODM) schema is read
through its eleven fixed fields, and `odm` derivations read one collected item
from it.

## Benchmarks

`python/check_benchmarks.py` runs every benchmark under `benchmarks/` through
the engine and compares each output, or each pinned failure, with its
`expected/` directory:

```bash
uv run --project python --no-sync python python/check_benchmarks.py
```

`python/tests/test_benchmarks.py` runs the same comparison under pytest.

## Not yet implemented

- Specification composition (`parents:`) and runs in which one specification
  reads another's output (#1504).
- The warning log and verification log sidecars (#1498).

## Design

[`DESIGN.md`](DESIGN.md) describes the engine's modules and how a
specification flows through validation, row construction, column derivation,
and verification.
