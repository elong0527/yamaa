# yamaa Rust workspace

The shared engine prepares specifications, builds domains and renders submission
documents for the Python and R packages. Language contracts live in
[rules/](../rules/README.md), syntax and schemas in [yaml/](../yaml/README.md),
and executable examples with authored expected results in
[benchmarks/](../benchmarks/README.md).

The [contributor guide](../CONTRIBUTING.md) owns crate boundaries, locked builds
and local verification commands. The
[release guide](../docs/release.md) owns supported platforms, approved decisions
and migration notes. Declared exports are read from the host sources
themselves; there is no committed API inventory. Current rule checks and
schema-owner controls are described in the [contributor guide](../CONTRIBUTING.md#develop-and-verify).
[Installed package checks](qualification/README.md) compare actual Python and R
results with authored expected data. The workflow recipes in
[.github/workflows/](../.github/workflows/) define the platform matrix and
packaging checks. Host usage and installation commands live in
[python/README.md](../python/README.md) and [R/yamaa/README.md](../R/yamaa/README.md).

Expected benchmark data and Parquet fixtures are authored truth. Preserve their
bytes when changing implementation or repository structure.

Layer composition and null handling follow the
[composition contract](../rules/specification/composition.md).

The adapter enables independent `yaml`, `parquet` and `readstat` Cargo features
by default. Python wheels, Python source installations and R source packages use
that default and retain every supported format. Rust embedders can select only
the physical formats they need:

```toml
yamaa-adapters = { path = "rust/crates/yamaa-adapters", default-features = false, features = ["yaml"] }
```

| Feature | Dependency edge and capability |
| --- | --- |
| `yaml` | Saphyr parser and adapter scalar decoding; admits specification, schema, environment, project-lock and style YAML bytes. |
| `parquet` | Parquet source and artifact encoding, schema metadata, and Brotli, Gzip, LZ4, Snappy and Zstd codecs. Zstd builds native C through `zstd-sys` and `cc`. |
| `readstat` | Bundled read-only C backend and direct `cc` build dependency for `.sas7bdat` and `.xpt` input. |
| No format features | CSV and Arrow table/IPC representation remain available, including supplied Arrow batches. No YAML parser, Parquet codec, ReadStat or native compression build is selected. |

Arrow arrays, buffers, schemas, IPC and FlatBuffers are representation and host
transfer dependencies shared by every configuration. They do not enable a
physical reader. The semantic `yamaa-core` crate remains independent of all
these codecs and representation libraries; callers can supply owned documents,
plans and tables to its ports without a physical reader.

Format APIs and their public error types remain available with features omitted.
YAML admission returns `yaml_decode::DecodeFailure::Unavailable`, Parquet input
returns `parquet_source::Error::ReaderUnavailable`, Parquet output returns
`parquet_artifact::Error::Unavailable`, and SAS input returns
`sas_source::Error::Unavailable`. These capability failures do not become
malformed-source language findings. Capture and ingest boundary replies expose
`yaml_reader_unavailable`, `parquet_reader_unavailable` and
`sas_reader_unavailable`; unavailable Parquet compression retains its separate
`parquet_codec_unavailable` code.

The optional-format CI matrix runs no-default, each individual feature and all
three features. Each lane exercises enabled-format fixtures and disabled
capabilities, verifies the Cargo graph, and inspects the linked test binary for
ReadStat and Zstd symbols. Run a lane locally from the repository root with
`python -m tools.ci.format_features none` (or `yaml`, `parquet`, `readstat`,
`all`). YAML-dependent specification and project fixture suites run whenever
`yaml` is selected; Arrow/CSV/IPC controls remain active without it. Ordinary
Python/R checks and native wheel/source qualification retain the packaged default.

Arrow/Parquet release-specific operations live in the adapter's
`storage_api` module: fallible IPC schema conversion, logical timestamp/integer
construction, and logical-type inspection. The canonical lock and manifests
continue to select 60; using 57 requires adapting this boundary and qualifying
that dependency graph. Keep malformed-schema rejection and the closed output
profile when adapting the boundary.
## Arrow host handoff

The host bridge uses Arrow-rs 60.0.0 C Data/Stream callbacks, PyArrow >=23.0.1
and R nanoarrow >=0.8.0. An exported stream owns a reader with an independent
cursor and shared immutable Arrow array references. Each emitted C Data array
owns its own buffer references. Dropping the build, stream or exporter cannot
invalidate arrays already imported by the consumer. Releasing a stream early
drops unread batches; imported batches retain their buffers. Moving a header
clears its source release callback. The current owner calls release exactly
once, and destruction of moved-from headers only frees the shell.

Python uses a named `arrow_array_stream` PyCapsule. PyO3 owns its header shell;
capsule destruction releases an unconsumed stream, or frees an already consumed
shell. R fills a fresh `nanoarrow_allocate_array_stream()` header. Nanoarrow
owns that allocation and invokes the Arrow-rs release callback on explicit
release or garbage collection, including after a failed conversion. Low-level
consumers should use an exit handler to release a stream they stop consuming.
No callback calls back into
Python or R, so buffer destruction requires neither interpreter nor build
handle. Native consumers/exporters must obey the Arrow ABI; forged pointers
are outside this memory-safety contract.

The zero-copy promise covers retained public Arrow buffers through standard
stream import/export, including sliced and chunked arrays. Build result
materialization, Polars conversion and the established R data-frame views can
copy. Public dates project Date32 days and zone-free datetimes project whole
TimestampSecond values, preserving existing public semantics; collected
precision remains private to execution. Import accepts UTF-8 (including large
and view arrays), Int64, Float64, Date32 and zone-free TimestampSecond. Other
types, fractional units and timezones require an explicit preserving conversion
before import. Nonfinite floats become missing under the existing engine rule.
Imported public temporals share their value buffers and allocate only a complete
precision child; public exports share the value buffer and parent validity.
R's exact i64 text and NUL raw-list views remain explicit copying conversions;
conversion to an incapable R vector preserves the value or raises an error.
Requested Python schema conversion is refused rather than silently narrowed.

