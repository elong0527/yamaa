# Installed table interchange

The optional native packages expose `table_round_trip(bytes)` and
`table_snapshot(bytes)`. Python uses owned `bytes`, R uses owned raw vectors.
Both call the same Rust adapter and require no Python process or R Arrow package
in the R path. These functions exchange tables; they do not execute specifications
or change the default backend. The installation capability flag remains false.

Input is one uncompressed, little-endian Arrow IPC V5 **stream**, with a schema,
zero or more record batches, and an explicit end marker. File format, dictionaries,
extension/custom metadata, compression, other versions, and trailing data are
rejected. Column names must be unique and nonempty. Physical schema is exactly
`arrow_table::physical_schema`: nullable Utf8, Int64, Float64, or a nullable
Struct with non-null `value` and `precision` children. Date uses Date32 with
precision UInt8 codes year=0/month=1/day=2. Datetime uses zone-free TimestampSecond
with precision codes day=0/second=1. Bare native dates/timestamps cannot substitute
for the precision-bearing internal representation.

The transport owns fixed policy: 8 MiB input and IPC output, 16 MiB snapshot JSON,
64 KiB per message metadata, 65,536 total rows, 64 columns, 256 batches, and 262,144
logical cells. Logical visible text across all cells is limited to 8 MiB, including
repeated references to aliased buffers. These are transport resource outcomes,
not yamaa language limits. FlatBuffers verification also caps nesting, table count
and apparent metadata size. Framing, body availability, schema shape, node counts,
null counts and buffer ranges are preflighted before Arrow decoding can allocate
from declared sizes. Arrow's normal validation remains enabled. No unsafe skip
validation, zero-copy host FFI, or process-abort immunity is claimed.

`table_round_trip` returns fresh IPC preserving column/row/chunk order, including
empty chunks and zero-row schemas. A schema-only stream stays schema-only;
zero-column batches retain row counts. Nonfinite floats become actual nulls.
Exports rebuild every array from visible logical cells: masked strings and
out-of-slice bytes are discarded, missing primitive payloads become zero, and
missing temporal children receive zero placeholders under the null parent.
This is the sanitizing public boundary; the internal `ArrowTable::batches()`
accessor alone is not. Output is capped while writing and IPC output can be
submitted again within the same byte policy. Returned data owns its buffers.

`table_snapshot` emits a compact `table/1` JSON object:

- `columns`: ordered `[name, type]` pairs.
- `row_count`: decimal string, never a host integer sentinel.
- `chunks`: ordered decimal row-count strings.
- `rows`: row-major cells using the established scalar codec, including integer
  decimal strings, exact float-bit strings, explicit missing and temporal precision.

Serialization streams through a capped writer rather than creating an expanded
JSON tree. Errors distinguish input limit, output limit, shape limit, invalid
stream, invalid table and internal failure. Messages do not echo raw input or
panic payloads. Python raises ValueError for rejection and RuntimeError for an
internal failure. R returns normally from native code before its facade raises
an error on the calling R thread. Repeated calls retain no prior result state.

## Python and Polars

`yamaa.adapters.native_tables` provides explicit `polars_from_ipc(request)` and
`ipc_from_polars(frame, schema)` helpers. Keep the canonical Arrow schema alongside
a Polars frame: Polars changes Utf8 to LargeUtf8, timestamps from seconds to finer
units, and temporal child nullability. It can also merge chunks. The helpers
preserve logical rows and columns; direct IPC interchange preserves chunks.

Restoration permits only equivalent physical representations. It never parses
text into numbers, promotes an integer column to float, invents precision, or
truncates fractional seconds. Only temporal children already masked by a null
parent receive placeholders; visible missing children fail. Timestamp casts are
safe and zone-free. Host frames are already materialized and serialization happens
before the native input budget; use the bounded native byte APIs for external
untrusted streams. Polars cannot represent a positive row count with zero columns;
use Arrow IPC/raw vectors for that shape.

## Qualification evidence and remaining work

The committed Arrow inputs are authored with PyArrow 25.0.1 using
`tools/build_table_inputs.py`. Expected JSON is separately hand-written; the TSV
contains equivalent compact UTF-8 truth for R without a JSON-package dependency.
The generator writes inputs only and never invokes Rust or regenerates expected
truth. Rust, installed Python wheel/source, and installed R source packages replay
these same expectations. Installed tests also read native output with PyArrow and
exercise Polars 1.44.2 with the locked Python dependencies and a non-editable facade
wheel. Malformed framing, every truncated prefix, absurd declared sizes, invalid
buffers, alias expansion, output escaping, masked secrets, missing temporal
children and fractional seconds have independent negative tests.

Pins arrow-ipc 60.0.0 (Apache-2.0, MSRV 1.88) and flatbuffers 25.12.19 (Apache-2.0,
MSRV 1.51) in adapters only. IPC defaults/compression are disabled; its transitive
graph adds Arrow selection/comparison support and FlatBuffers build helpers.
Core/engine remain free of Arrow and host types. Cargo transitive locking, process
safety, synchronous callbacks, strict shared specification compilation, workflow
execution, verification/publication, all dataset benchmarks and release/default
cutover qualification remain separate gates.
