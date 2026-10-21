# Independent Parquet inputs

These files were authored by PyArrow 25.0.1 using
`tools/staging/build_parquet_source_inputs.py`. They are inputs, not regenerated
expected outputs. Rust tests specify the exact scalar values independently.

The 24 ordinary containers vary six codecs, both page versions and dictionary
encoding. `delta` also exercises delta integer/string and byte-stream-split
encoding. Values include full i64, signed zero, the smallest binary64 subnormal,
empty/NUL/Unicode text, missing values, and both civil calendar bounds. Separate
inputs exercise empty tables, nonfinite normalization and diagnostic precedence.
The three `utf8*` containers have one deliberately corrupted string payload.
Ten unsupported-type containers pin full type names independently read by
PyArrow, including nested and fixed-size-list shapes, timestamp units/timezones
and decimal precision. Seven more, authored by PyArrow 26.0.0, pin the types
that the pinned Arrow and Parquet releases do not know, as PyArrow 26.0.0
reads them: 32- and 64-bit decimals and list views, which only the embedded
Arrow schema names, and the GEOMETRY, GEOGRAPHY and VARIANT logical types.
PyArrow cannot write VARIANT, so the builder annotates the group of a struct
it wrote in the footer. `date64` pins the writer's physical DATE normalization.
Four mixed flat/nested inputs pin each nested root's own field diagnostic and
the earlier empty/duplicate-name precedence, including maps and two-child structs.

`ordered-sum` changes only the original ordered-sum source's storage container.
Its field declarations are explicit in the generator, and its build continues to
use the committed benchmark's independently authored complete report and CSV.
