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

## Resolved specification artifact

`resolved_artifact::ResolvedArtifact` is an owned inspection format produced from
the same prepared documents and producer graph used for direct compilation.
Version 1 serializes as compact UTF-8 JSON with these envelope fields:

| Field | Meaning |
| --- | --- |
| `format` | `yamaa.resolved-specification`, distinct from authored YAML. |
| `artifact_version` | Integer `1`; readers reject other versions. |
| `model_kind` | `normalized_with_named_dictionary_references`. |
| `replay` | `inspection_only`. |
| `entry` | Canonical identity of the requested specification entry. |
| `environment` | Explicit environment identity, or null when absent. |
| `entries` | Root first, then linked producer entries in preparation order. |

Each entry contains `schema_version`, its complete composed normalized
`specification`, `layers` in composition postorder, `origins`, `mapping_order`,
`resources`, and `producers` mapping input dataset names to their producer entry.
Named mapping-dictionary references remain in the specification; preparation
admits those resources as usual, and their declaring locations are recorded.
The artifact carries no reusable executable code or activated environment.
Normalized inherited paths may already be respelled relative to the entry;
resource records retain their original spellings and selected locations. The
normalized mapping must not be dumped as authored YAML and compiled again.

`origins` uses RFC 6901 JSON pointers to identify normalized values. Each origin
records its original `declaring_source`, `source_kind` (`authored`,
`schema_default`, or `composed`) and whether normalization generated it.
Composed containers with no single declaring layer have a null source. Default
origins identify the retained schema module. `mapping_order` uses the same
pointers to retain every mapping's original key order; JSON object keys alone
cannot retain semantic iteration order. Arrays retain their normal order.

Resources include final input data and producer links, output and both logs,
named dictionaries and every inherited-parent reference. Each record contains
`spec_path`, `role`, `owning_entry`, `declaring_source`, `relative_to`,
`base_source`, `written`, and `location: {kind: local, path: ...}`. The composed
selector governs inherited input paths and schema links; output is atomic.
Every producer selects its own linked entry. Locations follow the existing
lexical local-path policy, including absolute and parent-relative spellings.
They state neither existence nor file-access authority. Environment internals
are admitted through the usual environment owner and are outside this domain
specification resource inventory.

Serialization is deterministic for identical preparation facts. Exact integer
tokens and floating-point values survive JSON round trips; mapping order has its
own ledger. Reading validates the distinct format and supported version and
retains these facts without source access, path resolution, callbacks or
recompilation. It works after source deletion and working-directory changes.
Loading does not authenticate caller-edited JSON. Resources contain no content
digests, bytes, file descriptors or access capabilities. Execution replay is
unsupported: `domain` and `check` continue to accept original source entries and
obtain fresh resource authority through their existing preparation lifecycle.
