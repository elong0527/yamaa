# Held Parquet source decoding

The optional original-YAML build path now selects CSV or Parquet from the authored
source extension in the core compiler. It does not inspect bytes to choose a
profile. Redundant Parquet field declarations produce `redundant_field_type`
under REQ-0533 before a study-data capture. Unsupported producer/schema and ODM
paths still fail preflight; this does not qualify a workflow or the complete
`schema-parquet` benchmark.

`yamaa-adapters::parquet_source::parse` consumes held bytes and returns an owned,
canonical Arrow table. It performs no filesystem operation, host call or
reference fallback. The engine's existing capture/decode/bind/execute service
retains those bytes, source observations and typed findings through failed builds.
The compiler owns the input empty-string policy; storage preserves empty text,
then the decoder applies that policy before binding and execution. CSV's existing
missing-string behavior is unchanged.

The stored mapping is the closed REQ-1031/1032 profile: String BYTE_ARRAY, plain
INT64, plain DOUBLE, Date INT32, and unadjusted microsecond Timestamp INT64.
Timestamps must represent exact whole seconds and both temporal types must stay
inside years 1..9999. Full i64, signed zero, subnormal floats, nulls, NUL and Unicode
text survive. Nonfinite floats become missing. Canonical temporal arrays retain
full collected precision instead of discarding it at the storage boundary.

The reader first admits Compact metadata, flattened schema depth and child counts,
page ranges/counts and embedded Arrow metadata. It bounds actual decompression,
decoder windows, dictionary expansion and delta reconstruction before invoking
the general physical readers. Every physical chunk, including unsupported fields,
is read before closed field/type admission. Temporal findings precede UTF-8
validation. No unchecked Arrow string is constructed. Fixed-list structural
checks read levels without allocating nested arrays.

Trusted limits separately bound source/metadata bytes, nodes/depth, rows/cells,
pages, decompression, expansion, retained arrays and canonical staging. These are
prototype resource policies, separate from language conditions. They do not promise
total RSS, allocator-failure recovery or support for arbitrary Parquet containers.
The installed source path currently admits 8 MiB of held source bytes across a
build, 65,536 rows and 262,144 cells per source, and separate 64 MiB decoded,
expanded and retained charges. General allocation inside upstream decoders remains
a release qualification concern. LZO produces an explicit codec-unavailable
boundary rejection; it is not mislabeled as malformed data or claimed as qualified.

The adapter pins Parquet/Arrow 60.0.0, bytes 1.12.1, base64 0.23.1, brotli 9.0.0,
flate2 1.1.10, lz4_flex 0.14.0, zstd 0.14.0 and snap 1.1.2. Compression dependencies
and Parquet's Arrow feature remain confined to adapters. Zstd introduces its native
build dependencies; transitive Cargo reproducibility is still tracked in #1742.
The dependency guard checks the new direct edges. No lockfile or provenance digest
is added by this slice.

Permanent independently authored PyArrow inputs exercise six codecs, both page
versions, dictionaries, delta integer/string and byte-stream-split encoding,
empty tables, exact boundary values, normalization and multi-fault precedence.
Rust tests compare exact values and precision. Both installed host tests change
only the ordered-sum input container and retain the original complete report,
verification observations and exact 1,030-byte CSV. Preparation/decode failures
expose no output and cannot publish; repeated save reads no source again. Python
blocks reference semantic imports during the build, and R removes Python from PATH.
These are supplemental integration gates, not promotion of the benchmark inventory
to `shared_run`, public facade qualification or default cutover.
