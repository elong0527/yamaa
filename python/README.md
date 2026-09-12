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

## Typed values and scalar expressions

The runtime value kernel exposes strict Pydantic result models, explicit
missingness, R011 conversions, R016 date and datetime values, an ordered Polars
table contract, and R004 predicate evaluation. Scalar dispatch currently
supports the normalized `source`, `literal`, and inline `mapping` expressions.
Other valid operations return an explicit `UnsupportedResult` until their
owning runtime components are implemented.

```python
from yamaa.expressions import MappingResolver, evaluate_expression
from yamaa.models import ValueResult, convert_value
from yamaa.specification.models import Expression

resolver = MappingResolver({"RAW.AGE": "42"})
expression = Expression(root={"source": {"variable": "RAW.AGE"}})
source = evaluate_expression(expression, resolver)

assert isinstance(source, ValueResult)
age = convert_value(source.value, "int")
assert age == ValueResult(value=42)
```

Run this component's focused tests from the repository root:

```bash
uv run --project python --isolated --extra test pytest \
  python/tests/models python/tests/expressions
```

## CSV source ingestion

Create one resource manager for the approved project and use normalized
`DatasetSource` declarations to load ordered, typed Polars tables:

```python
from yamaa.ingest import load_source_tables
from yamaa.resources import ProjectResources
from yamaa.specification.models import DatasetSource

resources = ProjectResources("study")
datasets = {
    "DM": DatasetSource(
        path="input/dm.csv",
        types={"AGE": "int", "RFSTDTC": "date"},
    )
}
loaded = load_source_tables(datasets, resources)
dm = loaded["DM"].table
```

The reader accepts the fixed `.csv` profile only. It parses the retained byte
snapshot in memory, preserves source row and header order, and distinguishes a
bare empty field from quoted empty text before applying declared types. It
does not create CSV or other intermediate files. Producer-linked `schema`
workflow resolution and header-contract comparison remain part of the workflow
component; this ingestion API rejects a declaration carrying `schema` until
that component supplies its resolved producer contract.

Run the focused resource and ingestion tests from the repository root:

```bash
uv run --project python --isolated --extra test pytest \
  python/tests/resources python/tests/ingest
```
