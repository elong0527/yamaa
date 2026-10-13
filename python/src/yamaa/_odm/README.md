# ODM file helpers

The retained file APIs read CDISC ODM clinical items without evaluating a
specification:

```python
from yamaa import read_odm, write_odm_parquet

frame = read_odm("input.xml")
written = write_odm_parquet("input.xml", "clinical-items.parquet")
print(written.output_path, written.row_count)
```

Both accept a plain XML file or a TAR archive. One XML member is selected
automatically; use `archive_member="path/to/odm.xml"` to select from several.
The helpers enforce archive member/expanded-byte and XML-depth limits. External
entities and unsupported clinical layouts are rejected.

The fixed 30-column schema projects study, subject, event, form, item-group and
item context. Metadata names use exact `(StudyOID, MetaDataVersionOID, OID)`
matches; missing metadata leaves a name null. `SourceOrdinal` is an unsigned
64-bit value starting at one in XML source order. `Value` is text or null;
`ValuePresent` and `IsNull` preserve absent, explicit-null and empty values.
No clinical value is inferred or repaired.

ODM 1.3 and 2.0 documents with `FormData` are supported. Without `FormData`,
two nested item groups supply form and item-group context. Unsupported layouts
fail explicitly. Empty documents still produce the complete typed schema.

`read_odm` returns a Polars data frame. `write_odm_parquet` appends bounded
batches into a temporary file and publishes one Parquet file atomically.
It refuses an existing output unless `overwrite=True`, and an unsuccessful
read leaves the prior output intact. The returned result has `output_path`
and `row_count`; the Parquet metadata records `yamaa.schema` as
`odm-clinical-item/1` and the row count. There is no intermediate CSV.

`yamaa._odm` owns private parsing/schema/writer code. Its internal iterator,
row models, schema constants and exceptions are not additional public
authoring or semantic APIs. Domain evaluation of an `odm` expression remains
the shared engine's responsibility.

The retained assertion suites are `test_readers.py`, `test_parquet.py` and
`test_schema.py` under `python/tests/odm`. The installed supplemental suite
`rust/tests/installed_odm_helpers.py` exercises both public helpers with ODM
1.3/2.0, exact values, null/empty distinctions, row order, schema metadata
and refusal to overwrite, while blocking reference imports.
