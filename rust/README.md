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

The adapter's default `readstat` feature builds the bundled read-only C backend
for `.sas7bdat` and `.xpt` input. Rust embedders that use only CSV and Parquet can
build `yamaa-adapters` with `default-features = false`; this omits the
ReadStat build and the adapter's direct `cc` build dependency. Parquet's Zstd
backend still builds its native library through a transitive `cc` dependency.
SAS input then returns
`sas_source::Error::Unavailable` (`sas_reader_unavailable` in boundary replies).
Python and R packages retain the default backend and all supported input formats.

Arrow/Parquet release-specific operations live in the adapter's
`storage_api` module: fallible IPC schema conversion, logical timestamp/integer
construction, and logical-type inspection. The canonical lock and manifests
continue to select 60; using 57 requires adapting this boundary and qualifying
that dependency graph. Keep malformed-schema rejection and the closed output
profile when adapting the boundary.
# Arrow host handoff

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
release, failed conversion or garbage collection. No callback calls back into
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

