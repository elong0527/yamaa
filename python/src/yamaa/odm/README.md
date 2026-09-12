# ODM helpers

The ODM module provides three general-purpose functions:

```python
from yamaa.odm import iter_odm_records, read_odm, write_odm_parquet
```

- `iter_odm_records()` streams strict Pydantic rows in XML source order.
- `read_odm()` returns the complete fixed-schema Polars DataFrame.
- `write_odm_parquet()` writes bounded Polars batches into one atomic Parquet
  file and returns its path and row count.

The functions accept a plain ODM XML file or a TAR archive. An archive with one
XML member is selected automatically. Pass `archive_member="path/to/odm.xml"`
when an archive contains more than one XML member.

Names are enriched from exact `(StudyOID, MetaDataVersionOID, OID)` metadata
matches when available. Missing metadata leaves the applicable name null; the
helpers contain no study aliases or repair policies.

When `FormData` is present, it supplies form context and its `ItemGroupData`
child supplies item-group context. Without `FormData`, two nested item groups
are supported: the outer group supplies form context and the inner group
supplies item-group context. Other layouts raise `ODMError`.

All tabular construction and validation use Polars. PyArrow is used only to
append bounded Polars batches to the staged Parquet file. The helper path
creates no CSV or intermediate dataset.

```python
from yamaa.odm import write_odm_parquet

result = write_odm_parquet("input.xml", "clinical-items.parquet")
print(result.row_count)
```

Run the focused tests from the installed, locked package environment:

```bash
uv sync --project python --extra test --locked --no-editable
uv run --project python --no-sync pytest python/tests/odm
```
