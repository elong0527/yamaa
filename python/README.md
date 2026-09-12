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

## ODM source binding and contextual resolution

Build one binding plan from a normalized specification and its loaded source
tables, then share one index across row-local resolvers:

```python
from yamaa.odm import BindingIndex, build_binding_plan

plan = build_binding_plan(loaded_spec.specification, loaded_sources)
index = BindingIndex(plan, loaded_sources)
resolver = index.context({"ODM": current_odm_row}, {"STUDYID": "STUDY01"})
```

The resolver implements the scalar expression protocol. Direct qualified
fields read the supplied source record, unqualified names read completed output
values, and a long-form ODM item uses every context column present in the ODM
projection. Duplicate ODM items require a structured R008 `multiple_matches`
policy; successful duplicate selection is returned with
`handled_by="multiple_matches"` so the executor can count that path.
Implicit cross-dataset row selection remains the keyed-join component's
responsibility; this context resolves only source records its caller has
explicitly bound.

Run this component's focused tests from the repository root:

```bash
uv run --project python --isolated --extra test pytest \
  python/tests/odm
```

## CSV source ingestion

Create one resource manager for the approved project and use normalized
`DatasetSource` declarations to load ordered, typed Polars tables:

```python
from yamaa.io import ProjectResources, load_source_tables
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
snapshot in memory, preserves source row and header order, and reads a field
with no characters as missing whether it was written bare or quoted, before
applying declared types. A `str`, `int`, or `float` column lands in the
matching native Polars type, a `date` column in `pl.Date`, and a `datetime`
column in `pl.Datetime("us")`, so an ingested table answers ordinary Polars
expressions. The reader does not create CSV or other intermediate files.
Producer-linked `schema` workflow resolution and header-contract comparison
remain part of the workflow component; this ingestion API rejects a declaration
carrying `schema` until that component supplies its resolved producer contract.

The R023 syntax scanner in `yamaa.io.csv` imports the standard library alone.
The repository validator loads that module by path rather than keeping a second
reader, so one implementation decides how every fixture reads.

Run this component's focused tests from the repository root:

```bash
uv run --project python --isolated --extra test pytest python/tests/io
```

## Verified tables and published artifacts

Assert over a completed table, then write and publish what it produces:

```python
from yamaa.io import ArtifactTarget, build_artifact, publish_artifact
from yamaa.verification import verify_completed_table

verify_completed_table(table, spec.columns, spec.keys, spec.verifications or [])
artifact = build_artifact(table, spec.output, spec.keys)
publish_artifact(ArtifactTarget(run_directory / "adsl.csv"), artifact)
```

`yamaa.verification` exposes one hook per R005 stage -- `check_column`,
`check_keys`, and `check_dataset` -- so an executor runs each assertion when
R005 says it runs rather than sweeping every check to the end. Each reports
failures in the committed error shape and leaves the run's fate to its
caller; `verify_completed_table` runs the three in order and raises. Dataset
verification accepts typed, per-row record-lookup bindings for the qualified
fields R004-26 makes visible to predicates.

`yamaa.io` writes the other way for the same reason it reads: the artifact
selects its profile from `output.path`, takes R005's column selection and
row order, and becomes `.csv` byte for byte or `.parquet` under the R020
type mapping. Publication replaces one target the caller explicitly
permits, through a temporary file beside it, so a failure leaves the
previous artifact in place.

See the [input and output documentation](src/yamaa/io/README.md) for what
each step owns.

Run this component's focused tests from the repository root:

```bash
uv run --project python --isolated --extra test pytest \
  python/tests/verification python/tests/io
```
