# YAMAA Python

The Python package currently provides general CDISC ODM helpers. It uses
Pydantic for public data contracts and Polars for tabular data operations.
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

## ODM helpers

```python
from yamaa.odm import iter_odm_records, read_odm, write_odm_parquet

frame = read_odm("input.xml")
result = write_odm_parquet("input.xml", "clinical-items.parquet")
```

See the [ODM helper documentation](src/yamaa/odm/README.md) for the fixed schema
and supported XML layouts.

## Specification loader

Load one specification against the repository schema bundle:

```python
from yamaa.specification import load_specification

loaded = load_specification("study/spec.yaml", "yaml")
print(loaded.specification.domain)
```

The loader applies YAML 1.2 core scalar rules, rejects YAML features outside
the authored-source contract, reads safe schema includes, validates the document
against the bundle, materializes R006 shorthands and defaults, and returns strict
Pydantic models. It does not execute the specification or resolve inheritance.
