# Independent Parquet inputs

These files were authored by PyArrow 25.0.1 using
`rust/tools/build_parquet_source_inputs.py`. They are inputs, not regenerated
expected outputs. Rust tests specify the exact scalar values independently.

The 24 ordinary containers vary six codecs, both page versions and dictionary
encoding. `delta` also exercises delta integer/string and byte-stream-split
encoding. Values include full i64, signed zero, the smallest binary64 subnormal,
empty/NUL/Unicode text, missing values, and both civil calendar bounds. Separate
inputs exercise empty tables, nonfinite normalization and diagnostic precedence.
The three `utf8*` containers have one deliberately corrupted string payload.
Ten unsupported-type containers pin full type names independently read by
PyArrow, including nested and fixed-size-list shapes, timestamp units/timezones
and decimal precision. `date64` pins the writer's physical DATE normalization.

`ordered-sum` changes only the original ordered-sum source's storage container.
Its field declarations are explicit in the generator, and its build continues to
use the committed benchmark's independently authored complete report and CSV.
