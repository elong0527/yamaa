# yamaa Rust workspace

The shared engine prepares specifications, builds domains and renders submission
documents for the Python and R packages. Language contracts live in
[rules/](../rules/README.md), syntax and schemas in [yaml/](../yaml/README.md),
and executable examples with authored expected results in
[benchmarks/](../benchmarks/README.md).

The [contributor guide](../CONTRIBUTING.md) owns crate boundaries, locked builds
and local verification commands. The
[release guide](../docs/release.md) owns supported platforms, approved decisions
and migration notes. The [shared host interface](interface/README.md) owns
`interface/bindings.yaml`, the one input from which both hosts' registrations
and declarations are generated; `make check-repository` and the native
qualification lane compare the generated files with it. Current rule checks and
schema-owner controls are described in the [contributor guide](../CONTRIBUTING.md#develop-and-verify).
[Installed package checks](qualification/README.md) compare actual Python and R
results with authored expected data. The workflow recipes in
[.github/workflows/](../.github/workflows/) define the platform matrix and
packaging checks. Host usage and installation commands live in
[python/README.md](../python/README.md) and [R/yamaa/README.md](../R/yamaa/README.md).

Expected benchmark data and Parquet fixtures are authored truth. Preserve their
bytes when changing implementation or repository structure.

Layer composition and null handling follow the
[composition contract](../rules/inheritance.md).

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

This section is the design note for #2726. It settled the public Rust API
before implementation; it changes no language semantics and no benchmark data.
The facade crate and host migration (#2733), the committed API inventory
`crates/yamaa/public-api.txt` and its check (#2734), and the rustdoc examples
with the in-memory example program `crates/yamaa/examples/memory.rs` (#2735)
are implemented.

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
    pub source_span: Option<SourceSpan>,     // byte span in the expression
    pub operand_route: Option<Vec<String>>,  // root-to-node operand steps
}
pub struct SourceSpan { pub start: usize, pub end: usize }  // #[non_exhaustive]
pub struct Report { pub diagnostics: Vec<Diagnostic> }  // #[non_exhaustive]
```

This is the public form of `issue_rows::Issue` (phase, condition, requirement,
spec paths, structured context and optional geometry). `DiagnosticContext` is
a structured value map, not a formatted string: `ContextValue` keeps integer
tokens exact (`Int`, `Uint`, or `Wide` with the decimal spelling of a token
outside both 64-bit ranges), fraction/exponent tokens as `Float`, and nested
lists and maps; `to_json()` returns the canonical text the host frames show.
Geometry is optional and absent means unrecorded: a present zero-width span or
empty route is a fact, never a default. The boundary version and record
definition in #2674 conform to this struct; the cross-host parity tests are
#2677.

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

## Partition-safety inventory and whole-table equivalence

This section is the specification for #2683 (step A of #2600; steps B-F are
#2684-#2688). It classifies every operation family and verification `Check`
variant in `yamaa-core` dataset execution, defines exact equivalence between
partitioned execution and whole-table execution, and identifies the seams where
partition boundaries sit. It changes no code behavior.

### Terminology: key groups vs. data partitions

On `main` today, the word "partition" in `table_grouping::partition`
(`rust/crates/yamaa-core/src/table_grouping.rs`) and `windows::Run`
(`rust/crates/yamaa-core/src/dataset/execution/windows.rs`) means a **key
group**: an equivalence class of row indices (`Vec<usize>`) sharing equal bound
column values within one materialized table snapshot, ordered by first source
occurrence.

In the partitioned and streaming execution contract (#2600, #2683-#2688), a
**data partition** (or **input partition**) is one table chunk $P_i$ in an
ordered sequence of primary-source batches $P_0, P_1, \dots, P_{k-1}$ ($k \ge
1$) whose row-wise concatenation $T = P_0 \mathbin{\Vert} P_1 \mathbin{\Vert}
\dots \mathbin{\Vert} P_{k-1}$ forms the logical primary input table. Secondary
lookup sources (`secondary: &[&dyn TableAccess]`) are immutable whole-table
snapshots shared across all primary partitions in the run.

### Classification vocabulary

Every operation family and verification check is classified into one of three
categories over an arbitrary (un-clustered) stream of primary data partitions:

- **`safe`**: Evaluates independently per primary data partition $P_i$ (and per
  candidate row within $P_i$) using only the immutable `DatasetPlan` and
  immutable secondary tables. Concatenating the per-partition outputs in
  partition order reproduces the whole-table output directly.
- **`safe-with-combine-step`**: Evaluates per partition $P_i$ while updating a
  bounded, named per-dataset accumulator (`HandlerCounter`, key-group state, or
  `CheckRecord` partials), followed by a deterministic combine/finalize step
  once all partitions have been consumed.
- **`global`**: Requires visibility of the complete primary table, completed
  cross-template `SELF` rows, or all candidate rows across partitions before
  evaluation can complete (for example, un-co-partitioned window sorting/ranking
  or primary-table self-joins). In partitioned execution (#2686), any plan
  containing an operation classified as `global` must be refused explicitly with
  a typed diagnostic unless whole-table materialization fallback is explicitly
  selected.

### Operation family inventory

#### 1. Row construction modes, drivers, and source binding

Key files: `rust/crates/yamaa-core/src/dataset/execution/mod.rs`,
`row_drivers.rs`, `key_grain.rs`, `lookup.rs`, `joined.rs`, `functions.rs`,
`scalar.rs`, `numeric.rs`, `flags.rs`, `imputation.rs`, `conversion.rs`,
`odm.rs`; `rust/crates/yamaa-core/src/table_grouping.rs`.

| Operation / construct | Location | Classification | Partition state and notes |
| --- | --- | --- | --- |
| `RowMode::Records` (primary driver) | `execution/mod.rs` (`process_template`) | `safe` (or `safe-with-combine-step` with handlers) | Each row $j$ in $P_i$ forms a single-member candidate `[base_i + j]`. Pre-window row-local filters run per row; `SelectionIdentity::Candidate` advances monotonically across partitions. |
| `RowMode::Keys` (`key_grain::construct`) | `execution/key_grain.rs`, `table_grouping.rs` | `safe-with-combine-step` (key/collect/reduce plans) / `global` (unprojected random primary reads) | Source filter and key assignments run per row. Distinct non-missing key tuples accumulate in first-occurrence order across partitions; each missing-key record remains an independent candidate at its global row position so `OutputKeys` reports it. `Collect`, `Count`, and `Reduce` fold per key group across partitions. Plans that later index arbitrary unprojected primary rows via `candidate.members` require whole-table state (`global`). |
| `RowMode::Groups(keys)` | `execution/mod.rs`, `table_grouping.rs` | `global` (arbitrary partitions) / `safe-with-combine-step` (group-accumulator fold or key-disjoint partitions) | On arbitrary partitions, a key group in `table_grouping::partition` can span multiple $P_i$, and template assignments/post-window filters read `candidate.members`. Safe per partition when inputs are co-partitioned by `keys`, or combinable when all member reads reduce through per-group accumulators. |
| `RowMode::Driver { source, mode }` | `execution/mod.rs`, `row_drivers.rs` | `global` (run-once secondary driver) | Drives candidates from `secondary[source - 1]`, not the primary table. Must execute once per dataset run rather than repeating per primary partition. |
| `RowMode::IntermediateDriver { index, source, mode }` | `execution/mod.rs`, `intermediates/mod.rs` | `global` | Consumes `intermediates::driver(index)` over the full donor relation; runs once per dataset when backed by a secondary source and requires full primary/`SELF` state otherwise. |
| `Expression::Literal`, `Expression::Column` | `execution/mod.rs` (`evaluate_leaf`) | `safe` | Pure candidate-local reads with no source or cross-row state. |
| `Expression::Source(column)` | `execution/mod.rs` (`evaluate_leaf`) | `safe` | Reads `candidate.members[0]` in the active partition; cell errors translate `source_row` to `base_i + local_row`. |
| `Expression::SourceBinding` | `execution/mod.rs` | `safe-with-combine-step` | Row evaluation delegates to `source` or returns the `absent` literal; `HandlerCounter` (`absent`, `multiple_matches`) combines across partitions by checked addition. |
| `Expression::FirstAvailable` | `execution/mod.rs` (`evaluate_first_available`) | `safe` (row-local operands) / `safe-with-combine-step` (with `Collect` or handlers) | Evaluates operands in authored order per candidate; combines handler counts and any per-key `SelectionRead::Collect` state. |
| Pure scalar and string/temporal expressions (`Compute`, `Round`, `Extreme`, `StrTemplate`, `StrExtract`, `StrCase`, `StrPad`, `Imputation`, `Temporal`, `Flag`, `Case`) | `execution/scalar.rs`, `numeric.rs`, `flags.rs`, `imputation.rs`, `str_*.rs`, `case_predicates.rs`, `conversion.rs` | `safe` (pure) / `safe-with-combine-step` (with recovery handlers or `Collect`) | Value computation is row-local when bindings are `Read::Column` or `Read::Source`. Recovery counts (`Missing`, `Invalid`, `Unconvertible`) in `HandlerCounter` combine by checked sum in registration order. |
| `Expression::Lookup`, `Expression::RowLookup`, `Expression::SourceLookup` | `execution/lookup.rs` | `safe` (no selection handler) / `safe-with-combine-step` (with `selection`) | Matches candidate keys or driver fields against the immutable whole-table `secondary[lookup.source]`. When `selection` resolves `count > 1`, `HandlerKind::MultipleMatches` combines in `HandlerCounter`. |
| `Expression::Joined`, `Expression::JoinedSource` | `execution/joined.rs`, `lookup.rs` | `safe-with-combine-step` (`Secondary`) / `global` (`Primary`) | Secondary joins scan immutable `secondary[index]` and combine `MultipleMatches` handler counts. Primary self-joins (`AggregateSource::Primary`) scan all primary rows and are `global`. |
| `Expression::Collect`, `Expression::Count`, `Expression::Reduce` | `execution/key_grain.rs`, `table_reduction.rs` | `safe` (`RowMode::Records`) / `safe-with-combine-step` (grouped/key-grain) | In `Records` mode `candidate.members` has length 1. Across partitions in `Keys`/`Groups` mode, `Count` sums `usize`, `Reduce` merges `Sum`/`Mean`/`Min`/`Max` accumulators, and `Collect` merges distinct present `Key` sets plus the best `(order_by, global_source_row)` extremum and `MultipleMatches` handler count. |
| `Expression::Function`, `Expression::ProjectFunction` | `execution/functions.rs`, `function_batch.rs` | `safe` (row-local inputs) / `safe-with-combine-step` (with `FunctionInput::Collect`) | `functions::admit_plan` runs once at `Admission`. Invocations run in deterministic global candidate order with failure `RowIdentity.position = out_base_i + local_row`. |
| `Expression::Odm` | `execution/odm.rs` | `global` (arbitrary partitions) / `safe-with-combine-step` (key-scoped or hierarchy-partitioned) | `odm::evaluate` scans the source (or `candidate.members` under `OdmScope::Keys`) to enforce strict item uniqueness (`OdmFinding::NotUnique`). On arbitrary partitions, non-key scopes are `global`; under `OdmScope::Keys` or hierarchy-co-partitioned inputs, selected values and `IDENTIFYING` sets combine per scope key. |

#### 2. Mapping and cut

Key files: `rust/crates/yamaa-core/src/mapping_cut.rs`,
`rust/crates/yamaa-core/src/dataset/execution/mapping_cut.rs`.

| Operation / construct | Location | Classification | Partition state and notes |
| --- | --- | --- | --- |
| `Mapping::new`, `Cut::new`, `Cut::declaration_findings` | `mapping_cut.rs` | `safe` | Plan-admission validation (`MappingAmbiguousDictionary`, `CutDeclaration`); runs once before data access. |
| `Expression::MappingCut` (`Mapping::select`, `Cut::select`) | `mapping_cut.rs`, `execution/mapping_cut.rs` | `safe-with-combine-step` | Dictionary matching and `breaks.partition_point` interval selection are pure functions of a single cell. Reached `missing` and `unmapped` handlers record into `HandlerCounter`, which combines across partitions by checked addition. Fatal diagnostics (`MappingMissing`, `MappingUnmapped`, `MappingInput`, `CutMissing`, `CutInput`, and unconvertible handler literals) attach `RowIdentity` at global position `out_base_i + local_row`. |

#### 3. Intermediates

Key files: `rust/crates/yamaa-core/src/dataset/execution/intermediates/mod.rs`,
`intermediates/key_index.rs`, `intermediates/match_values.rs`,
`self_relations.rs`.

| Operation / construct | Location | Classification | Partition state and notes |
| --- | --- | --- | --- |
| `IntermediateSource::Input(index)` (uncorrelated filter, `unique`, `StringIndex`, uncorrelated `derivations`) | `intermediates/mod.rs`, `key_index.rs` | `safe-with-combine-step` | Depends only on immutable `secondary[index]`. `intermediates::verify(..., false, ...)` runs once before primary partitions; `eligible[&index]`, `string_indexes[&index]`, and uncorrelated `materialized[&(index, None)]` are computed lazily once and shared across all partitions. Per-candidate `choices` and `HandlerCounter` (`NoMatch`, `MultipleMatches`, nested `Unconvertible`) combine across partitions. |
| `IntermediateSource::Input(index)` (correlated `filter`, `range`, or correlated `derivations` where `!derived.current.is_empty()`) | `intermediates/mod.rs`, `match_values.rs`, `relation_predicates.rs` | `safe-with-combine-step` | Evaluates current-row operands per candidate and scans/materializes against the full immutable `secondary[index]`. Cache entries keyed by `SelectionIdentity` are candidate-local and dropped at partition end; handler counts combine across partitions. |
| `IntermediateSource::Primary` (all reads, `driver`, `materialize`, and `unique` verification) | `intermediates/mod.rs` | `global` | Scans `0..primary.row_count()` to check uniqueness, evaluate eligibility, count matches, and break ties by global donor row index. Cannot execute on a single primary partition without whole-table primary state. |
| `IntermediateSource::SelfRows` (`SELF` donors, `Run::publish_self`, `verify(..., true, ...)`) | `intermediates/mod.rs`, `self_relations.rs` | `global` | `Donors` borrows completed candidates from prior `RowTemplate`s (`&candidates[..first]`), and `verify(..., true, ...)` partitions all `SELF` rows to enforce `IntermediateUnique`. Requires all prior templates' global candidates. |
| Windowed intermediate match keys (`MatchValue::Expression` with `expression.windowed`, `Run::begin_columns`) | `intermediates/mod.rs`, `match_values.rs` | `global` | Key values depend on a window over the constructed relation; `begin_columns` invalidates template-phase caches because column-phase windows observe the complete output relation. |
| `intermediates::check_unobserved_filter_types` | `intermediates/mod.rs` | `safe` | Inspects static deferred filter-type findings on `plan.intermediates()` after column completion; independent of partition count. |

#### 4. Aggregates

Key files: `rust/crates/yamaa-core/src/dataset/execution/aggregates.rs`,
`rust/crates/yamaa-core/src/aggregate_reduction.rs`.

| Operation / construct | Location | Classification | Partition state and notes |
| --- | --- | --- | --- |
| `AggregateRelation::CurrentGroup` (within single-partition candidates, e.g., `RowMode::Records` or key-disjoint partitions) | `execution/aggregates.rs` | `safe` (or `safe-with-combine-step` with `derive_records` handlers) | All `candidate.members` reside in the active partition $P_i$; `select`, `derive_records`, and `reduce_selected` complete locally and merge `HandlerCounter`. |
| `AggregateRelation::CurrentGroup` (groups split across arbitrary partitions) | `execution/aggregates.rs` | `safe-with-combine-step` (pure reducers) / `global` (correlated primary/output derivations or non-reduced representative reads) | Reducers (`Count`, `Sum`, `Mean`, `Min`, `Max`, `SingleValue`) and input-type checks combine across partitions per group; plans whose `AggregateResult::Numeric` reads an unreduced donor column from `representative` or whose `derive_records` performs global lookups require full group materialization (`global`). |
| `AggregateRelation::Matched { source: AggregateSource::Secondary(index), .. }` | `execution/aggregates.rs` | `safe-with-combine-step` | Matches each candidate against immutable `secondary[index]`, runs `derive_records` and `reduce_selected` locally, and combines `HandlerCounter`. |
| `AggregateRelation::Matched { source: AggregateSource::Primary, .. }` | `execution/aggregates.rs` | `global` | Scans `0..primary.row_count()` for every candidate row to select matching primary donors and reduce them. |
| `AggregateRelation::Output { group_by }` | `execution/aggregates.rs` | `global` | Scans `0..state.output_rows.len()` across all constructed output candidates matching `group_by`. |

#### 5. Windows and output ordering

Key files: `rust/crates/yamaa-core/src/dataset/execution/windows.rs`,
`row_windows.rs`, `ordering.rs`.

| Operation / construct | Location | Classification | Partition state and notes |
| --- | --- | --- | --- |
| `Expression::Window`, `Expression::ContextWindow` (`WindowKind::BaselineFlag`, `RowValue`, `Locf`, `RowNumber`, `Competition`, `Dense`) | `execution/windows.rs`, `row_windows.rs` | `global` (arbitrary partitions) / `safe-with-combine-step` (only when input partitions are disjoint on non-empty `window.group_by` and preserve within-group order) | `windows::Run::new_schema` partitions the entire candidate relation by `window.group_by` (or places all rows into one group when `group_by` is empty), sorts members by `window.order_by` with global candidate index tie-breaking (`left.cmp(&right)`), evaluates `window.filter` in sorted order, and computes ranks, row-offset lookups, carry-forward values, or `BaselineFlag` / `BaselineAmbiguity`. Over arbitrary partitions, all window kinds are `global`. |
| `ordering::apply` (`DatasetPlan::output_order`) | `execution/ordering.rs`, `execution/mod.rs` | `global` (or `safe-with-combine-step` via global k-way merge after verification) | Runs after `OutputKeys` and `Verification` succeed. Sorts accepted rows by `output_order` terms using pre-sort global candidate index `left.cmp(&right)` as the final tie-breaker. |

#### 6. Output key checks and verification `Check` variants

Key files: `rust/crates/yamaa-core/src/dataset/mod.rs`,
`rust/crates/yamaa-core/src/dataset/execution/mod.rs`, `verification.rs`,
`rust/crates/yamaa-core/src/dataset_checks.rs`.

| Check / variant | Phase & location | Classification | Combine state and notes |
| --- | --- | --- | --- |
| `OutputMissingKeyFailed` (`keys[position]` completeness) | `OutputKeys` (`execution/mod.rs`) | `safe-with-combine-step` | Per key column, each partition records offending global row identities (`out_base_i + local_row`); combine sums `evaluated_count` and `failed_count`, sets `output_rows = N`, and concatenates `offending_rows`. |
| `OutputDuplicateKeyFailed` (`keys` uniqueness) | `OutputKeys` (`execution/mod.rs`, `table_grouping.rs`) | `safe-with-combine-step` | Accumulates distinct key tuples across partitions with first-seen group order, first member `RowIdentity`, and member count. At finalize, `evaluated_count` is total distinct key groups, `failed_count` is groups with count $> 1$, and `offending_rows` lists `members[0]` for each duplicate group in first-seen group order. |
| `IntermediateUnique` (`intermediates::verify`) | `Derivation` (`intermediates/mod.rs`) | `safe-with-combine-step` (`Input`) / `global` (`Primary`, `SelfRows`) | Secondary-source `unique` checks run once against immutable `secondary[index]`. Primary and `SELF` `unique` checks require the global donor table. |
| `Check::NotMissing` | Column / `Verification` (`execution/mod.rs`, `dataset_checks.rs`) | `safe-with-combine-step` | Row-local `matches!(value, Value::Missing)`; combine sums `evaluated_count` and `failed_count`, sets `output_rows = N`, and concatenates `offending_rows` in global row order. |
| `Check::AllowedValues(values)` | Column (`execution/mod.rs`, `dataset_checks.rs`) | `safe-with-combine-step` | Row-local membership test against fixed `values`; `CheckRecord` combines by addition/concatenation across partitions. |
| `Check::Codelist { id, values }` | Column (`execution/mod.rs`, `dataset_checks.rs`) | `safe-with-combine-step` | Row-local `compare_present` test against `values`; combines `CheckRecord` counts, `offending_rows`, and `CodelistObservation { id, values }`. |
| `Check::ValueCodelists { discriminator, overrides, bindings }` | Column (`execution/mod.rs`, `dataset_checks.rs`) | `safe-with-combine-step` | Runs once both the target column and `discriminator` are complete on the candidate. Emits one `CheckRecord` per binding in `bindings` order; each binding's counts, `offending_rows`, and `CodelistObservation.values` combine across partitions. |
| `Check::Inventory { values, scope }` | Column (`execution/mod.rs`, `dataset_checks.rs`) | `safe-with-combine-step` | Row-local check gated by optional `scope.discriminator` on the same candidate; combines counts, `offending_rows`, and `inventory: Some(Vec<Value>)`. |
| `Check::ValueMaxLength { max, discriminator, codes, include }` | Column (`execution/mod.rs`, `dataset_checks.rs`) | `safe-with-combine-step` | Row-local `text.chars().count()` bound check gated by `discriminator`; `CheckRecord` combines across partitions. |
| `Check::Range { min, max }` | Column (`execution/mod.rs`, `dataset_checks.rs`) | `safe-with-combine-step` | Row-local numeric range comparison against `min`/`max`; `CheckRecord` combines across partitions. |
| `Check::MaxLength(usize)` | Column (`execution/mod.rs`, `dataset_checks.rs`) | `safe-with-combine-step` | Row-local Unicode scalar count check `text.chars().count() > max`; `CheckRecord` combines across partitions. |
| `Check::WideMaxLength(RowCountBound)` | Column (`execution/mod.rs`, `dataset_checks.rs`) | `safe-with-combine-step` | Row-local `max.compare_count(text.chars().count()) == Ordering::Less`; `CheckRecord` combines across partitions. |
| `Check::Matches(Pattern)` | Column (`execution/mod.rs`, `dataset_checks.rs`) | `safe-with-combine-step` | Row-local regex search with per-cell `Resource::RegexWork` step guard; `CheckRecord` combines across partitions (or aborts at the first `ScopedLimit`). |
| `Check::InvalidDiagnostic(Diagnostic)` | Column / `Verification` (`execution/mod.rs`) | `safe` | Data-independent deferred diagnostic; fires at its declaration checkpoint with the prefix `records` accumulated before that checkpoint. |
| `Check::InvalidDeclaration { code, reason }` | Column / `Verification` (`execution/mod.rs`) | `safe` | Data-independent deferred compiler finding; fires at its declaration checkpoint with the prefix `records` accumulated before that checkpoint. |
| `Check::PredicateDeclaration(BoundPredicate)` | `Verification` (`execution/verification.rs`) | `safe` | Schema-only sample validation (`validate_predicate`); reads no candidate rows and emits no `CheckRecord`. |
| `Check::Assert { when, require }` | `Verification` (`execution/verification.rs`) | `safe-with-combine-step` (column / secondary-intermediate reads) / `global` (primary or `SELF` intermediate reads) | `validate_predicate` runs once on synthetic schema samples; each candidate evaluates `when` and `require` without short-circuiting. Counts and `offending_rows` combine across partitions unless a bound intermediate is `global`. |
| `Check::AllOrNone(columns)` | `Verification` (`execution/mod.rs`) | `safe-with-combine-step` | Row-local check that all `columns` share the same missingness on each candidate; `CheckRecord` counts and `offending_rows` combine across partitions. |
| `Check::Unique(columns)` | `Verification` (`execution/mod.rs`, `table_grouping.rs`) | `safe-with-combine-step` | Tracks distinct column-tuple keys across partitions with first-seen group order and all member `RowIdentity`s per group. At finalize, `evaluated_count` is total distinct groups, `failed_count` is groups with $> 1$ member, and `offending_rows` flattens repeated groups in first-seen group order, then within-group global row order. |
| `Check::RowCount { min, max }` | `Verification` (`execution/mod.rs`) | `safe-with-combine-step` | Sums output row counts $N = \sum_i |P_i^{\text{out}}|$ across partitions and evaluates `row_count_accepts(N)` once at finalize (`evaluated_count = 1`, `output_rows = N`). |
| `Check::WideRowCount { min, max }` | `Verification` (`execution/mod.rs`) | `safe-with-combine-step` | Sums output row counts $N = \sum_i |P_i^{\text{out}}|$ across partitions and compares against `RowCountBound` once at finalize (`evaluated_count = 1`, `output_rows = N`). |
| `Check::ScopedRowCount(RowCount)` | `Verification` (`execution/verification.rs`) | `safe-with-combine-step` (column / secondary-intermediate predicates) / `global` (primary or `SELF` intermediate predicates) | Evaluates `filter` (`admitted`) and `when` (`bound`) per row and folds into per-`group_by` tuple accumulators (or one global group when `group_by` is `None`) storing `(first_seen_order, first_row_identity, total_members, admitted_count, any_bound)`. Filtered-empty groups remain counted in `evaluated_count` unless exempted by `when.is_some() && !any_bound`. Finalize checks `accepts(admitted_count, total_members)` in first-seen group order and populates `offending_rows`, `counts`, and optional `denominators`. |

### Whole-table equivalence specification

Let $T$ be a primary source table with schema $S$ and $R$ rows, and let $P_0,
P_1, \dots, P_{k-1}$ ($k \ge 1$) be any partition of $T$ into tables with schema
$S$ and row counts $r_0, r_1, \dots, r_{k-1}$ such that $\sum_{i=0}^{k-1} r_i =
R$ and $P_i.\text{cell}(j, c) = T.\text{cell}(\text{base}_i + j, c)$ for all $i
\in [0, k)$, $j \in [0, r_i)$, and columns $c$, where $\text{base}_i = \sum_{m <
i} r_m$. Secondary tables and the function port are identical in both runs.

Partitioned execution over $(P_0, \dots, P_{k-1})$ satisfies **whole-table
equivalence** with `DatasetPlan::execute(T, secondary, functions, observer)` if
and only if all five properties below hold for every partition split (including
empty partitions $r_i = 0$ and single-row partitions $r_i = 1$):

1. **Values and representation**:
   - On success, the partitioned `Dataset.schema` and `Dataset.rows` are equal
     (`PartialEq`) to the whole-table `Dataset.schema` and `Dataset.rows`. Every
     cell matches in logical `Value`, including canonicalized `FiniteFloat` bit
     patterns, exact `Date` and `DateTime` components, private collected
     temporal precision metadata prior to public projection, and UTF-8 string
     bytes.
   - Exported public `Table` schemas, Arrow batch columns, validity bitmaps, and
     saved artifact bytes match the whole-table output byte for byte.

2. **Row-order guarantees**:
   - **Candidate construction order**: Before presentation sorting, output rows
     are ordered by `(template_index, global_first_occurrence)`:
     - In `RowMode::Records`, retained rows appear in strictly increasing global
       source index $\text{base}_i + j$.
     - In `RowMode::Keys` and `RowMode::Groups`, groups appear in ascending
       order of their first feeding global source index $\min_{m \in
       \text{members}} m$ across $T$, with missing-key records in `RowMode::Keys`
       kept as individual candidates at their global source positions.
   - **Tie-breaking**: Every stability tie-breaker in source selection
     (`key_grain::select`, `intermediates::select`, `lookup::select`), window
     sorting (`windows::sort`), and presentation sorting (`ordering::apply`)
     compares global source row indices $\text{base}_i + j$ or global pre-sort
     candidate positions $\text{out\_base}_i + r$, never partition-local offsets.
   - **Presentation order**: When `output_order` is non-empty, `ordering::apply`
     runs after `OutputKeys` and `Verification` succeed and produces the exact
     stable merge-sort permutation of the whole-table candidate sequence.

3. **Diagnostic and error identity**:
   - **Phase and checkpoint precedence**: Execution preserves the whole-table
     phase order (`Admission` -> `Derivation` [eager secondary/primary
     intermediate `verify` -> template candidate construction -> column-by-column
     completion with interleaved `check_columns` prefix checkpoints ->
     `check_unobserved_filter_types`] -> `OutputKeys` -> `Verification` ->
     `ordering::apply` -> `Finished`). If whole-table execution fails,
     partitioned execution fails at the same phase and checkpoint with an
     identical `ExecutionError` variant, `ConditionCode`, `spec_paths`,
     `source_span`, `operand_route`, and structured `DiagnosticContext`.
   - **Global coordinate translation**:
     - Primary-source cell and predicate errors (`ExecutionError::Cell`,
       `ExecutionError::Predicate`) report `source_row = base_i + local_row`.
     - Output-row identities (`RowIdentity { position, values }` on execution
       errors and inside `CheckRecord.offending_rows`) report the global
       pre-sort candidate index `position = out_base_i + local_row` and the
       completed key `values`.
   - **Explicit refusal of `global` operations**: When a plan containing any
     `global` operation is executed through the partitioned surface without an
     explicit whole-table fallback (#2686), execution refuses with a typed
     diagnostic before publishing any output rather than computing
     partition-local approximations.

4. **Retained observations (`handler_counts` and `verifications`)**:
   - `ExecutionAttempt.handler_counts`: Handler sites are registered in
     deterministic plan order during `Admission`; per-site counts sum across
     partitions via checked addition in `HandlerCounter` (raising
     `ExecutionError::HandlerAccounting` on overflow) and match the whole-table
     snapshot on both success and failure at the same checkpoint.
   - `Execution.verifications` and `ExecutionAttempt.retained_verifications`:
     Every emitted `CheckRecord` equals the whole-table `CheckRecord` in `path`,
     `code`, `evaluated_count`, `failed_count`, `output_rows` (total dataset
     output rows $N$), `offending_rows` (ordered by global candidate position or
     first-seen group order), `codelist`, `inventory`, and `row_count`. On a
     verification failure, `retained_verifications` preserves the exact prefix
     of records completed before the failing check.

5. **Resource limits (`Limits`)**:
   - Table capacity guards (`Limits::table` for `SourceRows`, `SourceCells`,
     `OutputRows`, and `OutputCells`) are enforced against cumulative global
     source rows $\sum_{m \le i} r_m$ and cumulative global output candidates,
     producing the same `SourceLimit`, `Limit`, and `ScopedLimit` refusals and
     `ExecutionEvent::Refusal` observer events as whole-table execution.

### Architectural seams for partition boundaries

1. **Attempt driver seam (`rust/crates/yamaa-core/src/dataset/execution/mod.rs`)**:
   - Split `DatasetPlan::execute_inner` / `execute_admitted` (`mod.rs:752-878`)
     into three explicit stages:
     1. *Run admission*: `functions::admit_plan`, secondary-source schema and
        `Limits::table` checks, `HandlerCounter` site registration
        (`mod.rs:663-700`), and one-time `intermediates::verify(..., false,
        ...)` for `IntermediateSource::Input`.
     2. *Per-partition step*: Accept each primary partition $P_i$ with
        `(base_source_row, base_output_row)`, update cumulative `Limits::table`
        counters, construct partition candidates, and evaluate row-local /
        combinable column assignments. Poll the cancellation token (#2684) at
        `ExecutionEvent::Phase` transitions, between partitions, and between
        intermediates.
     3. *Combine and finalize*: Finalize cross-partition group/key state, run
        `check_unobserved_filter_types`, combine `OutputKeys` and
        `Verification` accumulators against total `output_rows = N`, and run
        `ordering::apply`.

2. **Grouping and key-grain seam (`rust/crates/yamaa-core/src/table_grouping.rs`, `rust/crates/yamaa-core/src/dataset/execution/key_grain.rs`)**:
   - `table_grouping::Key<'a>` (`table_grouping.rs:14-40`) borrows `&'a str`
     from a single table snapshot. Cross-partition grouping introduces an owned
     counterpart (`OwnedKey` / canonical `Vec<Value>` with `-0.0` canonicalized
     to `0` bits and temporal precision ignored in `Ord`/`Eq`, matching `Key<'a>`
     semantics) so group maps outlive individual partition borrows by direct
     value comparison.
   - In `key_grain::construct` and `key_grain::collect_bound`
     (`key_grain.rs:7-266`), replace retained whole-table row-index vectors
     (`Candidate.members: Vec<usize>`) with a per-key combine state holding:
     first global source row index, per-column `BTreeSet<OwnedKey>` of distinct
     present values plus best `(order_by_values, global_source_row)` for
     `Collect`, and running `(count, NumericReducer)` state for `Count` and
     `Reduce`.

3. **Intermediate cache lifecycle seam (`rust/crates/yamaa-core/src/dataset/execution/intermediates/mod.rs`, `key_index.rs`)**:
   - Separate `intermediates::Run` (`intermediates/mod.rs:18-23`) into two
     lifetimes:
     - *Run-scoped state* (retained across all primary partitions):
       `eligible[&index]`, `string_indexes[&index]`, and uncorrelated
       `materialized[&(index, None)]` for `IntermediateSource::Input(index)`.
     - *Partition-scoped state* (cleared after each partition $P_i$ finishes its
       column pass): `choices` (keyed by `(SelectionIdentity, usize)`) and
       correlated `materialized` entries (keyed by `(index,
       Some(SelectionIdentity))`). Clearing partition-scoped entries at each
       partition boundary bounds intermediate memory to the active partition and
       static secondary tables.

4. **Column-verification checkpoint seam (`rust/crates/yamaa-core/src/dataset/execution/mod.rs:1245-1308`, `1550-1772`)**:
   - `complete_columns` interleaves column derivation with `check_columns`
     prefix checkpoints so an error-severity column check at column $c$ preempts
     derivation failures in column $c + 1$.
   - In partitioned execution, per-column `CheckRecord` accumulators collect
     `(evaluated_count, failed_count, offending_rows, codelist, inventory)` per
     column checkpoint across partitions, while any derivation failure in a
     later column $c' > c$ is ordered after earlier column-$c$ check failures so
     whole-table failure precedence is preserved.

5. **Dataset verification combine seam (`rust/crates/yamaa-core/src/dataset/execution/mod.rs:1313-1547`, `verification.rs`)**:
   - Factor `verify_output` (`mod.rs:1313`) and `verification::predicate_check` /
     `verification::row_count_check` (`verification.rs:129-302`) behind a
     per-partition accumulator that ingests `(out_base_i, &[Candidate])` per
     partition and finalizes once with total `output_rows = N`:
     - Row-local checks (`OutputMissingKeyFailed`, `NotMissing`, `AllowedValues`,
       `Codelist`, `ValueCodelists`, `Inventory`, `ValueMaxLength`, `Range`,
       `MaxLength`, `WideMaxLength`, `Matches`, `Assert`, `AllOrNone`) append
       global `RowIdentity`s and sum counts.
     - Key-grouped checks (`OutputDuplicateKeyFailed`, `Check::Unique`,
       `Check::ScopedRowCount`) fold into `BTreeMap<Vec<OwnedKey>, GroupState>`
       preserving first-seen group order.
     - Whole-table count checks (`Check::RowCount`, `Check::WideRowCount`)
       evaluate `row_count_accepts(N)` at finalize.

