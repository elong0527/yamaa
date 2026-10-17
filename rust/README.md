# yamaa Rust workspace

The shared engine prepares specifications, builds domains and renders submission
documents for the Python and R packages. Language contracts live in
[rules/](../rules/README.md), syntax and schemas in [yaml/](../yaml/README.md),
and executable examples with authored expected results in
[benchmarks/](../benchmarks/README.md).

The [contributor guide](../CONTRIBUTING.md) owns crate boundaries, locked builds
and local verification commands. The
[release guide](../docs/release.md) owns supported platforms, approved decisions
and migration notes. The [shared host interface](interface/README.md) chooses
the generated boundary and inventories both hosts' declarations in one input;
registrations remain handwritten until the generators land. Current rule checks and
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

## Public Rust API design

This section is the design note for #2726. It settles the public Rust API
before implementation; it changes no language semantics and no benchmark data.
Implementation is tracked in #2733 (facade crate and host migration), #2734
(committed API inventory and its check) and #2735 (rustdoc and the in-memory
example program).

### Facade location and boundary

The supported API lives in one new crate, `rust/crates/yamaa` (package name
`yamaa`), a small facade over `yamaa-adapters` and `yamaa-core`. The owner
decided this on 2026-10-16: a separate facade crate, not a module inside
`yamaa-adapters`. Host crates depend only on the facade (plus their language
runtime), so the compiler enforces the boundary: the 64 `pub mod`s of
`yamaa-adapters` and the 103 of `yamaa-core` stay internal and cannot be
reached past the facade. The facade re-exports no adapters or core type; every
public type below is facade-owned, and adapters/core types are converted at
the boundary. Core keeps its current boundary (no I/O, no `unsafe`, no Arrow).

The Python and R hosts are the first clients (#2657 and #2689 target this one
definition). When a host needs something the facade lacks, that is a gap to
fill in the facade, not a reason to reach past it. The facade wraps the
existing prepare, bind, execute and publish workflow -- there is no second
admission or evaluation route. All physical access goes through a resolver;
local files are just the default resolver.

### Operations and signatures

An embedder configures one `Engine` and calls operations on it. Options and
request types are `#[non_exhaustive]` structs so fields can be added without
breaking callers.

```rust
pub struct Engine { /* resolver + host configuration, Arc-shared */ }

#[non_exhaustive]
pub struct EngineOptions {
    pub resolver: Option<Arc<dyn ResourceResolver>>,
    pub host_language: HostLanguage,
    pub runtime_name: String,      // folds in specification_report::Identity
    pub runtime_version: String,
    pub functions: Option<Arc<dyn ProjectFunctions>>,
    pub observer: Option<Arc<dyn Progress>>,
}

impl EngineOptions {
    pub fn new(
        host_language: HostLanguage,
        runtime_name: String,
        runtime_version: String,
    ) -> Self;
    pub fn with_resolver(mut self, resolver: Arc<dyn ResourceResolver>) -> Self;
    pub fn with_functions(mut self, functions: Arc<dyn ProjectFunctions>) -> Self;
    pub fn with_observer(mut self, observer: Arc<dyn Progress>) -> Self;
}

#[non_exhaustive]
pub struct Request<'a> {
    pub specification: &'a str,    // entry path or resolver reference
    pub environment: Option<&'a str>,
}

impl<'a> Request<'a> {
    pub fn new(specification: &'a str) -> Self;
    pub fn with_environment(mut self, environment: &'a str) -> Self;
}

impl Engine {
    pub fn new(options: EngineOptions) -> Engine;
    pub fn check(&self, request: &Request<'_>, token: &CancellationToken) -> Report;
    pub fn build(&self, request: &Request<'_>, token: &CancellationToken)
        -> Result<Build, Error>;
    pub fn define(&self, request: &DefineRequest<'_>, token: &CancellationToken)
        -> Result<DefineBuild, Error>;
    pub fn resolve_specification(&self, request: &Request<'_>)
        -> Result<ResolvedSpecification, Error>;
}

#[non_exhaustive]
pub struct DefineRequest<'a> {
    pub environment: &'a str,
    pub standard: &'a str,
    pub created: Option<&'a str>,
    pub installed: &'a str,
}

impl<'a> DefineRequest<'a> {
    pub fn new(environment: &'a str, standard: &'a str, installed: &'a str) -> Self;
    pub fn with_created(mut self, created: &'a str) -> Self;
}

pub struct Build { /* owned attempt state */ }
impl Build {
    pub fn output(&self) -> Option<&Table>;
    pub fn output_bytes(&self) -> Option<&[u8]>;
    pub fn log(&self, warning: bool) -> Option<&Table>;
    pub fn log_bytes(&self, warning: bool) -> Option<&[u8]>;
    pub fn report(&self) -> &Report;
    pub fn save(&mut self) -> Result<SaveOutcome, Error>;
}

pub struct DefineBuild { /* owned attempt state */ }
impl DefineBuild {
    pub fn report(&self) -> &Report;
    pub fn save(&mut self) -> Result<SaveOutcome, Error>;
}

pub enum SaveOutcome { Saved, NothingToSave }  // #[non_exhaustive]
```

- `EngineOptions`, `Request` and `DefineRequest` are `#[non_exhaustive]`, so
  external crates cannot use struct literals; the `new` plus `with_*`
  constructors above are the only way to build them, which keeps additive
  field changes non-breaking.
- `check` is infallible at the boundary, as today's `check_with_resolver` is:
  admission and preparation failures come back as diagnostics in the returned
  `Report`, never as a second error channel.
- `build` mirrors today's `Result<Domain, Failure>`: a rejected build returns
  `Error::Rejected(Report)`; a successful `Build` carries the same outputs,
  logs and observations as today's `Domain`.
- `save` is a method on the build handle, as `Domain::save` is today; it is
  the only mutating operation and publishes through the resolver.
- `define` mirrors `file_define::Define::prepare` plus its save step.
- `resolve_specification` stays public, as decided for 0.4.0 in #2725. The
  facade type is `ResolvedSpecification`; the internal `ResolvedArtifact`
  (above) is its implementation and wire format, not a public type.
- `compile` and `run` are reserved names, not part of the first surface. They
  land with the reusable plan handle (#2654, #2679-#2682) as
  `Engine::compile(&self, &Request<'_>) -> Result<Plan, Error>` and
  `Plan::run(&self, &RunRequest<'_>, &CancellationToken) -> Result<Build, Error>`,
  wrapping the existing `project_run::PreparedRun` instead of inventing a
  second plan type.

### Handles, ownership and thread safety

| Handle | Ownership | `Send` | `Sync` | Notes |
| --- | --- | --- | --- | --- |
| `Engine` | Owned, cheap to clone (shared `Arc` state) | Yes | Yes | Operations take `&self`; concurrent calls are independent attempts. |
| `Build`, `DefineBuild` | Owned, one attempt | Yes | No | `save(&mut self)` publishes once, as today. |
| `Plan` (future, #2654) | Owned, reusable | Yes | Yes | `run` takes `&self`; each run is an independent attempt. |
| `Table` | Owned, immutable batches | Yes | Yes | Borrowed views share buffers with the owning table. |
| `ResolvedSpecification` | Owned, immutable | Yes | Yes | No source access after construction. |
| `MemoryResolver` | Owned, interior-mutable | Yes | Yes | Mirrors today's `host_resources::MemoryResolver`. |
| `CancellationToken` | Owned, cheap to clone | Yes | Yes | All clones share one flag. |
| `dyn ResourceResolver`, `dyn ProjectFunctions`, `dyn Progress` | Caller-owned, shared as `Arc` | Required | Required | The facade never calls back into a host while holding an engine lock; there are no engine locks. |

Project functions supplied by a host are invoked on the calling thread of the
operation, as today; a host whose runtime is single-threaded (Python, R) owns
that constraint, exactly as the current host crates do.

### Results

`Table` is the facade form of `public_table::PublicTable`: owned Arrow
`RecordBatch`es with their schema, plus `columns()`, `rows()`, `ipc()` and
`stream()`. Stream export follows the ownership rules in "Arrow host handoff"
above unchanged. The Arrow type of each public column type is part of the
committed API inventory: strings are `Utf8`, integers `Int64`, floats
`Float64`, dates `Date32` (days) and zone-free datetimes `TimestampSecond`.
There is no boolean column type in the public schema vocabulary, so no
specification column ever surfaces as an Arrow `Boolean`.

### Diagnostics

Diagnostics are typed records, never JSON text or tuples:

```rust
pub struct Diagnostic {           // #[non_exhaustive]
    pub severity: Severity,       // Error | Warning
    pub phase: String,            // stage that produced the record
    pub code: String,             // stable condition code, e.g. `unknown_field`
    pub requirement: Option<String>,
    pub spec_path: Vec<String>,   // ordered path segments
    pub context: DiagnosticContext,
}
pub struct Report { pub diagnostics: Vec<Diagnostic> }  // #[non_exhaustive]
```

This is the public form of today's `issue_rows::Issue` (phase, condition,
requirement, spec paths, context). `DiagnosticContext` is a structured value
map, not a formatted string; the structured context and geometry work in
#2674-#2677 enriches its contents without changing this shape. The boundary
version and record definition in #2674 must conform to this struct; the
cross-host parity tests are #2677.

### Errors

One public error type at the operation boundary, converted inside the facade
from the internal errors consolidated per module in #2658:

```rust
pub enum Error {                  // #[non_exhaustive]
    Rejected(Report),             // the operation was refused with diagnostics
    Resolver(ResolverFailure),    // a caller-supplied resolver failed
    Host(FunctionError),          // a caller-supplied project function failed
    Table(TableError),            // Arrow/IPC representation failed
    Unavailable { feature: &'static str },  // a format feature is not compiled in
    Cancelled,                    // the cancellation token was cancelled
    Internal { code: &'static str },        // stable code, no internal payload
}
```

`ResolverFailure`, `FunctionError` and `TableError` are small facade-owned
types (also `#[non_exhaustive]`) carrying a stable code and a message. Internal
error enums -- `file_workflow::SelectionError`, `file_resources::Error`,
`yaml_decode::DecodeFailure`, the `file_define::Failure` family and the rest --
never appear in a facade signature; #2658's per-module consolidation is what
makes this conversion table finite.

### Cancellation

`CancellationToken` is a cloneable handle over a shared atomic flag, the same
pattern as `MemoryResolver::cancel` today:

```rust
impl CancellationToken {
    pub fn new() -> CancellationToken;
    pub fn cancel(&self);
    pub fn is_cancelled(&self) -> bool;
}
```

Hosts wire interrupts into the token (#2685). Threading the token through
`DatasetPlan::execute` is #2684; partial-result, publication and cleanup rules
on cancellation are #2688. The token shape is settled here so those issues do
not reopen it.

### Features

The facade forwards the existing format features to `yamaa-adapters`:
`yaml`, `parquet` and `readstat`, all on by default, and it builds with any
combination of them, as adapters does today in the format-feature CI matrix.
With no format feature, CSV and Arrow routes remain, exactly as described in
the feature table above. A capability a build omits fails the same way it does
today: the stable codes (`yaml_reader_unavailable`,
`parquet_reader_unavailable`, `sas_reader_unavailable`,
`parquet_codec_unavailable`) surface as diagnostics or as
`Error::Unavailable`. The facade adds no native dependencies of its own.

### Stability policy

- The facade is the only supported Rust surface. Adapters and core may change
  in any release without notice; their `pub` items are visibility for the
  workspace, not a public API.
- All public enums and option/request structs are `#[non_exhaustive]`.
- Between 0.x minor versions the facade may add types, variants and fields.
  It may not remove, rename or change the signature of a public item, except
  when a language-semantics change requires it -- and then the break is called
  out in the release notes for that version. Patch releases never break the
  facade.
- A committed API inventory of the facade crate is generated (for example
  with `cargo public-api`) and checked in CI (#2734): a surface change
  without a regenerated, reviewed inventory in the same change fails the
  check. There is deliberately still no inventory for adapters or core.
- The minimum Rust version is the workspace `rust-version` (1.90 today).
- Rustdoc carries a runnable example for each operation, and one example
  program runs a specification entirely from memory (#2735).

### Distribution

All crates, including the facade, stay `publish = false` for 0.4.0. Embedders
depend on the facade by git or path dependency:

```toml
yamaa = { path = "rust/crates/yamaa", default-features = false, features = ["yaml"] }
```

Publishing to crates.io is not decided here. It depends on #2583 -- the
workspace `[patch.crates-io]` vendored `saphyr-parser` cannot be published as
a patch -- and on the provenance terms of the vendored ReadStat C sources.
Until #2583 lands, git/path is the distribution route.

### Host-usage mapping

Every adapters or core module the two host crates use today (28 adapters
modules, 11 core modules, counted from the host sources on 2026-10-16) maps to
a facade item or is marked internal. Any module not listed here is internal by
default.

| Module used today | Facade disposition |
| --- | --- |
| `application_measurements` | Internal -- a host-side timing helper; the facade `Report` carries no measurements in the first surface. |
| `arrow_bridge` | Facade -- `Table::stream` and stream import on `Table`; the FFI types themselves are not re-exported. |
| `arrow_table` | Internal -- the engine's table representation; `Table` is the public form. |
| `callback` | Facade -- the `ProjectFunctions` trait and `FunctionError`; `BoundaryError`/`CallbackError` convert into `FunctionError`. |
| `file_application` | Facade -- `Engine::check`/`Engine::build`; `Application` and `ApplicationError` are internal. |
| `file_define` | Facade -- `Engine::define` and `DefineBuild::save`; its `Failure` converts into `Error`. |
| `file_graph` | Internal -- producer graph state behind `Build` and the future `Plan`. |
| `file_preparation` | Internal -- `FileSpecification` is a prepared internal form. |
| `file_producer_build` | Internal -- attempt state behind `Build`. |
| `file_project` | Internal -- project state behind `Build` and the future `Plan`. |
| `file_resources` | Internal -- its `Error` converts into `ResolverFailure`/`Error::Internal`. |
| `file_workflow` | Internal -- `SelectionError` converts into diagnostics/`Error`. |
| `host_resources` | Facade -- the `ResourceResolver` trait, `MemoryResolver`, and the resolver request/reply/failure types. |
| `ipc_table` | Internal -- `TableError` converts into the facade `TableError`; `Table::ipc` is the public IPC route. |
| `issue_rows` | Facade -- `Diagnostic` and `Report` are the public form of `Issue`; the module itself is internal. |
| `producer_attempt` | Internal -- host-failure traversal; failures surface as `Error::Host`. |
| `producer_result` | Facade -- `Build` is its public form (`project_owned` stays an internal constructor). |
| `project_application` | Internal -- selection failures convert into diagnostics. |
| `project_attempt` | Internal -- same disposition as `producer_attempt`. |
| `project_lock` | Facade -- `LockFinding`/`LockReason` are the public lock-finding types and `decode_renv_lock` is the public renv admission; lock-kind detection stays internal (the engine supplies `LockKind` through `LockContext`). |
| `project_report` | Internal -- `build_result` runs behind `Build`; `Classified` stays an internal constructor. |
| `project_run` | Facade (future) -- `Plan` wraps `PreparedRun` (#2679); `Attempt`/`PreparedRun` stay internal in the first surface. |
| `public_table` | Facade -- `Table`. |
| `resolved_artifact` | Facade -- `ResolvedSpecification` (#2725). |
| `scalar_bytes` | Internal -- scalar decoding is a conversion detail; project functions exchange facade `Scalar` values. |
| `specification_report` | Facade -- its `Identity` folds into `EngineOptions` (`runtime_name`/`runtime_version`); the module is internal. |
| `style_values` | Internal -- style comparison is applied inside the workflow from resolver-supplied resources. |
| `yaml_decode` | Internal -- `DecodeFailure` converts into diagnostics/`Error::Unavailable`. |
| `yamaa_core::application_observer` | Facade -- the `Progress` trait with facade `Phase`/`Outcome` enums; the `Scope` guard stays internal. |
| `yamaa_core::domain_entry` | Internal -- entry dispatch behind `Engine`. |
| `yamaa_core::function_batch` | Facade -- `ProjectFunctions` batch invocation with facade scalar arguments; `BatchFunctionPort` stays internal. |
| `yamaa_core::function_invocation` | Facade -- `FunctionError` is its public form; `HostError` stays internal. |
| `yamaa_core::function_signature` | Facade -- `FunctionIdentity` and `Signature`/`Parameter` are the public activation types; `LogicalSignature` stays internal. |
| `yamaa_core::project_activation::ActivationPort` | Facade -- the `ProjectFunctions` trait; the core port stays internal. |
| `yamaa_core::project_environment` | Facade -- `LockKind` and `LockContext`; environment admission stays internal. |
| `yamaa_core::project_function` | Facade -- `HostLanguage` and the facade `Scalar` value type; the core `Language` enum stays internal. |
| `yamaa_core::table::ValueRef` | Facade -- `ScalarRef`; the core table stays internal. |
| `yamaa_core::temporal` | Facade -- `Date` and `DateTime`; collected precision is dropped at the boundary. |
| `yamaa_core::value::Value` | Facade -- `Scalar`. |
| `yamaa_core::VERSION` (crate root) | Facade -- re-exported as `yamaa::VERSION` with the same value. |
