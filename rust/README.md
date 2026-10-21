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
`interface/bindings.yaml`, the one input from which both hosts' registrations,
declarations and public layers are generated; `make check-repository` and the
native qualification lane compare the generated files with it. Current rule checks and
schema-owner controls are described in the [contributor guide](../CONTRIBUTING.md#develop-and-verify).
[Installed package checks](qualification/README.md) compare actual Python and R
results with authored expected data. The workflow recipes in
[.github/workflows/](../.github/workflows/) define the platform matrix and
packaging checks. Python and R usage lives in the
[R and Python API](../docs/articles/api.md) page, and the installation commands
in [python/README.md](../python/README.md) and
[R/yamaa/README.md](../R/yamaa/README.md).

Expected benchmark data is authored truth. Preserve its bytes when changing
implementation or repository structure. The adapters' Parquet test inputs are
written when the tests run (`crates/yamaa-adapters/tests/support/parquet_files.rs`),
since binary files live only under `benchmarks/`.

Layer composition and null handling follow the
[composition contract](../rules/inheritance.md).

The `yamaa` facade crate forwards the adapter's independent `yaml`, `parquet`,
`readstat` and `odm` Cargo features, all enabled by default. Python wheels,
Python source installations and R source packages use that default and retain
every supported format. Rust embedders depend on the facade, never on
`yamaa-adapters`, and can select only the physical formats they need. This
selection admits YAML specifications and reads CSV and Parquet, and it omits
the bundled ReadStat C build, so it reads no SAS input:

```toml
yamaa = { path = "<yamaa checkout>/rust/crates/yamaa", default-features = false, features = ["yaml", "parquet"] }
```

[Distribution](#distribution) shows the dependency; the `yaml` feature needs no
other configuration.

| Feature | Dependency edge and capability |
| --- | --- |
| `yaml` | Saphyr parser and adapter scalar decoding; admits specification, schema, environment and project-lock YAML bytes, and the files `lint` checks. |
| `parquet` | Parquet source and artifact encoding, schema metadata, and Brotli, Gzip, LZ4, Snappy and Zstd codecs. Zstd builds native C through `zstd-sys` and `cc`. |
| `readstat` | Bundled read-only C backend and direct `cc` build dependency for `.sas7bdat` and `.xpt` input. |
| `odm` | The `quick-xml` tokenizer and the pure-Rust Gzip codec; reads CDISC ODM 1.3 and 2.0 XML clinical items from plain XML or plain/Gzip TAR files into the fixed clinical-item table, for `read_odm` and for a specification's `.xml` inputs. |
| No format features | CSV and Arrow table/IPC representation remain available, including supplied Arrow batches. No YAML parser, Parquet codec, ODM XML reader, ReadStat or native compression build is selected. |

Arrow arrays, buffers, schemas, IPC and FlatBuffers are representation and host
transfer dependencies shared by every configuration. They do not enable a
physical reader. The semantic `yamaa-core` crate remains independent of all
these codecs and representation libraries; callers can supply owned documents,
plans and tables to its ports without a physical reader.

Format APIs and their public error types remain available with features omitted.
YAML admission returns `yaml_decode::DecodeFailure::Unavailable`, Parquet input
returns `parquet_source::Error::ReaderUnavailable`, Parquet output returns
`parquet_artifact::Error::Unavailable`, SAS input returns
`sas_source::Error::Unavailable`, and ODM XML input returns
`odm_xml::Error::Unavailable`. These capability failures do not become
malformed-source language findings. Capture and ingest boundary replies expose
`yaml_reader_unavailable`, `parquet_reader_unavailable`,
`sas_reader_unavailable` and `odm_reader_unavailable`; unavailable Parquet
compression retains its separate `parquet_codec_unavailable` code.

The Release compatibility workflow's Rust job adds two minimal facade builds:
no optional formats and YAML + Parquet. Each compiles the adapter's unit tests,
runs the focused adapter capability tests, verifies the facade's Cargo graph, and
inspects the linked test binary for ReadStat and Zstd symbols. The core's
dependency isolation is checked once. Run both builds from
the repository root with `python -m tools.ci.format_features`; CI uses the
workspace's minimum Rust toolchain. Ordinary workspace tests cover all default
formats, including the full specification and project fixture suites. Python/R
checks and native wheel/source qualification retain the packaged default.

Arrow/Parquet release-specific operations live in the adapter's
`storage_api` module: fallible IPC schema conversion, C stream import,
row-group reader construction, logical timestamp/integer construction, and
logical-type inspection. The canonical lock and manifests select exactly
55.2.0, the newest release that builds with the minimum Rust; using another
release requires adapting this boundary and qualifying that dependency graph. Keep malformed-schema rejection and the closed output
profile when adapting the boundary.
## Arrow host handoff

The Python bridge uses Arrow-rs 55.2.0 C Data/Stream callbacks and PyArrow
>=23.0.1. The R binding instead copies each table into plain R vectors in Rust,
so no Arrow stream crosses the R boundary. An exported stream owns a reader
with an independent cursor and shared immutable Arrow array references. Each
emitted C Data array
owns its own buffer references. Dropping the build, stream or exporter cannot
invalidate arrays already imported by the consumer. Releasing a stream early
drops unread batches; imported batches retain their buffers. Moving a header
clears its source release callback. The current owner calls release exactly
once, and destruction of moved-from headers only frees the shell.

Python uses a named `arrow_array_stream` PyCapsule. PyO3 owns its header shell;
capsule destruction releases an unconsumed stream, or frees an already consumed
shell. Low-level consumers should use an exit handler to release a stream they
stop consuming. No callback calls back into Python or R, so buffer destruction
requires neither interpreter nor build handle. Native consumers/exporters must
obey the Arrow ABI; forged pointers are outside this memory-safety contract.

The zero-copy promise covers retained public Arrow buffers through standard
stream import/export, including sliced and chunked arrays. Build result
materialization, Polars conversion and R data-frame conversion can copy.
Public dates project Date32 days and zone-free datetimes project whole
TimestampSecond values, preserving existing public semantics; collected
precision remains private to execution. Import accepts UTF-8 (including large
and view arrays), Int64, Float64, Date32 and zone-free TimestampSecond. Other
types, fractional units and timezones require an explicit preserving conversion
before import. Nonfinite floats become missing under the existing engine rule.
Imported public temporals share their value buffers and allocate only a complete
precision child; public exports share the value buffer and parent validity.
R's data-frame conversion holds integers as doubles and text as character, and
raises an error for an integer beyond 2^53 or text containing NUL rather than
change the value. Requested Python schema conversion is refused rather than
silently narrowed.

## Resolved specification artifact

`resolved_artifact::ResolvedArtifact` is an owned inspection format produced from
the same prepared documents used for direct compilation.
Version 1 serializes as compact UTF-8 JSON with these envelope fields:

| Field | Meaning |
| --- | --- |
| `format` | `yamaa.resolved-specification`, distinct from authored YAML. |
| `artifact_version` | Integer `1`; readers reject other versions. |
| `model_kind` | `normalized_with_named_dictionary_references`. |
| `replay` | `inspection_only`. |
| `entry` | Canonical identity of the requested specification entry. |
| `project` | Explicit project identity, or null when absent. |
| `entries` | Exactly one element: the requested entry. |

The entry contains `schema_version`, its complete composed normalized
`specification`, `layers` in composition postorder, `origins`, `mapping_order`
and `resources`. A linked producing specification is a `producer_schema`
resource of the entry, never an entry of its own.
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

Resources include final input data and producer links, output and the report file,
named dictionaries and every inherited-parent reference. Each record contains
`spec_path`, `role`, `owning_entry`, `declaring_source`, `relative_to`,
`base_source`, `written`, and `location: {kind: local, path: ...}`. The composed
selector governs inherited input paths and schema links; output is atomic.
Locations follow the existing
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
runtime), so the compiler enforces the boundary: the 69 `pub mod`s of
`yamaa-adapters` and the 107 of `yamaa-core` stay internal and cannot be
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
    pub host_language: Option<HostLanguage>,  // none: a neutral engine
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
    pub fn neutral(runtime_name: String, runtime_version: String) -> Self;
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
    pub fn check(&self, request: &Request<'_>, token: &CancellationToken)
        -> Result<Report, Error>;
    pub fn build(&self, request: &Request<'_>, token: &CancellationToken)
        -> Result<Build, Error>;
    pub fn compile(&self, request: &Request<'_>, token: &CancellationToken)
        -> Result<Plan, Error>;
    pub fn resolve_specification(&self, request: &Request<'_>)
        -> Result<ResolvedSpecification, Error>;
    pub fn read_odm(&self, source: &str, options: &OdmOptions) -> Result<Table, Error>;
    pub fn save_table(&self, table: &Table, out: &str) -> Result<Vec<String>, Failure>;
    pub fn file_type(&self, path: &str) -> Result<FileType, Failure>;
}

#[derive(Clone)]
pub struct Plan { /* shared engine configuration and compiled state */ }
impl Plan {
    pub fn run(&self, request: &RunRequest, token: &CancellationToken) -> Result<Build, Error>;
    pub fn report(&self) -> &Report;  // a refused compilation; empty when compiled
}

#[non_exhaustive]
#[derive(Clone, Default)]
pub struct RunRequest {
    pub resolver: Option<Arc<dyn ResourceResolver>>,  // inputs and Save targets
    pub observer: Option<Arc<dyn Progress>>,          // this run's events
}

impl RunRequest {
    pub fn new() -> Self;
    pub fn with_resolver(mut self, resolver: Arc<dyn ResourceResolver>) -> Self;
    pub fn with_observer(mut self, observer: Arc<dyn Progress>) -> Self;
}

pub struct Build { /* owned attempt state */ }
impl Build {
    pub fn output(&self) -> Option<&Table>;
    pub fn output_bytes(&self) -> Option<&[u8]>;
    pub fn run_report(&self) -> Option<&Table>;
    pub fn run_report_bytes(&self) -> Option<&[u8]>;
    pub fn report(&self) -> &Report;
    pub fn save(&mut self) -> Result<Vec<String>, Failure>;  // the paths written
}
```

- `EngineOptions`, `Request` and `RunRequest` are
  `#[non_exhaustive]`, so external crates cannot use struct literals; the
  `new` plus `with_*` constructors above are the only way to build them, which
  keeps additive field changes non-breaking.
- `EngineOptions::new` claims a host language, as the Python and R hosts do.
  `EngineOptions::neutral`, which `EngineOptions::default()` also gives,
  claims none, for the command line and a bare Rust embedder (#3030 section
  2.10). It reads no project's `environments.<host>` entry, so it verifies no
  lock and activates nothing; a build that calls a project function is refused
  before any input is read as `unsupported_operation` with operation
  `environment`, and every other build runs as a host runs it.
- `check` reports admission and preparation failures as diagnostics in the
  returned `Report`, as today's `check_with_resolver` does, never as a second
  error channel. Only cancellation fails it: a cancelled call returns
  `Error::Cancelled`, with no report and no issue, as every operation does.
- `build` mirrors today's `Result<Domain, Failure>`: a rejected build returns
  `Error::Rejected(Report)`; a successful `Build` carries the same output,
  run report and observations as today's `Domain`.
- `save` is the one save every result shares: `Run`, `ProjectRun` and
  `Submission` save their outcome through it, and a build handle saves as an
  outcome of one member. It plans every file, opens and stages all of them,
  then commits them in publication order and returns their absolute paths with
  forward slashes (a caller's resolver names each by its target reference). A
  refusal is a `Failure` carrying its issues and the paths written before it.
  It and `save_table` are the only mutating operations, and both publish
  through the resolver.
- `save_table` saves a table, such as the item table `read_odm` returns, to
  a path its caller names, as the command line's `read-odm SOURCE OUT` does.
  The path's case-insensitive extension selects `csv` or `parquet` from
  OUTPUT-5's closed mapping, the encoders of a run's output write the table,
  and the one save's publisher stages and commits the file and returns its
  path. Its refusals are a run's: `unknown_artifact_profile` for another
  extension, and the save's `engine_rejected` codes for a target that cannot
  be written (#3030 D134).
- Every operation goes through the engine's resolver: checks, builds,
  plans, project reads and submissions, `resolve_specification`, `read_odm`,
  `save_table` and `file_type`.
  Without one, the engine's local file resolver serves the same requests, so a
  resolver that records them sees every access. A resolver serves a regular
  file or reports what else a name reaches with
  `ResolverFailure::from_kind(ResourceKind)`: a link of any sort, a junction
  or another reparse point included, fails as `resource_path_symlink`
  (FILE-8), a directory or other entry as `resource_path_not_regular_file`
  and a missing name as `resource_path_missing`. `MemoryResolver::insert_kind`
  models these.
- `read_odm` through a resolver captures the source once and holds it whole,
  under the 10 GiB per-file source bound (`source_odm_limit_exceeded`, `ODM
  source exceeds 10737418240 bytes`), before `max_expanded_bytes` applies:
  the resolver protocol has no streaming read. Without a resolver, a local
  archive is streamed and never held whole, and a local plain XML file over
  `max_expanded_bytes` is refused before it is read; one within that bound
  is held whole.
- `file_type` captures a file once and interprets only its `yamaa` and
  `type` header, returning the type it declares, which the command line
  dispatches on (#3030 D122). A file that declares `schema_version` has no
  type and is refused as `unsupported_language_version` at `schema_version`
  (D125).
- `resolve_specification` stays public, as decided for 0.4.0 in #2725. The
  facade type is `ResolvedSpecification`; the internal `ResolvedArtifact`
  (above) is its implementation and wire format, not a public type.
- `compile` and `run` are the reusable plan handle (#2821). `compile` is the
  preparation half of `build`: it captures and admits the metadata through
  the engine's resolver and compiles the specification, with its project when
  the request names one, reading no study data. `Plan::run` is the rest: activation, input capture, binding,
  execution and conversion through the same public tables. `build` is exactly
  one `compile` followed by one `run`, with the same events, so there is one
  admission and evaluation route. The plan wraps the one prepared
  specification, alone or with its project, rather than a second plan type.
- A refused compilation still returns a `Plan`: `report` holds its
  diagnostics, and each run returns that same failed `Build`, with its
  retained preparation sources, without resource access. Only cancellation
  fails `compile` or `run` itself; each token belongs to one call.
- Each run opens its own resource scope. Without a run resolver it reads
  inputs and targets Save where compilation did; a `RunRequest` resolver
  serves the inputs at the same declaring identities and receives the run's
  Save targets. Nothing a run captures, activates or produces stays in the
  plan, so a failed run leaves the plan ready for the next. A `Build` owns its
  run and outlives the plan.
- A plan never runs stale and never recompiles implicitly. Each `run` first
  captures every held metadata file again through the compile authority (the
  specification, its layers, dictionaries and linked producing
  specifications, the project file, the environment file and the lock,
  function definitions and codelists they declare) and compares it with the
  held reference and bytes, before activation or any study read. A file that
  changed refuses that run with `resource_path_content_changed` (FILE-10),
  naming the field that declared it and the path as written; a resolver
  object that is gone refuses it with `resource_path_missing`. Activation
  stays per run, so the host's current lock, bindings and conformance cases
  are checked against the compiled function contracts every time. Once the
  original is back, the next run succeeds; only an explicit `compile` picks
  up a change. `build` skips the run-start check because its compilation has
  just captured the same metadata.
- The hosts' lazy objects keep a `Plans` holder each (#2821).
  `Engine::spec_run_reusing` and `Engine::project_run_reusing` first resolve
  the request's paths as compilation does: absolute against the current
  working directory, with each parent directory canonical. They run the plan
  kept for those identities when the run-start check finds every file it was
  compiled from unchanged (#3069). Otherwise they compile again exactly as
  `build` does and keep the new plan; a refused compilation keeps nothing. A
  kept plan holds no project functions or observer. It runs through the
  calling engine's, so each host call keeps its own capabilities. `spec_run`
  and `project_run` run through an empty holder.
- Every host operation produces one outcome: its members in run order and
  the project-level issues that refused the whole operation. A specification
  run is a producer graph of one member; a project run is a graph of every
  specification in its scope, producer first, and a check or a submission has
  one member per specification or section. `Check`, `Run`, `ProjectRun` and
  `Submission` are views of it. Each computes its `Status` from its issues and
  what it built, and never stores it (P-STATUS-1): `succeeded`, `failed`, and
  `skipped` for a project-run member whose run-scope producer failed or was
  skipped.

### Handles, ownership and thread safety

| Handle | Ownership | `Send` | `Sync` | Notes |
| --- | --- | --- | --- | --- |
| `Engine` | Owned, cheap to clone (shared `Arc` state) | Yes | Yes | Operations take `&self`; concurrent calls are independent attempts. |
| `Build` | Owned, one attempt | Yes | No | `save(&mut self)` publishes once, as today. |
| `Plan` | Owned, reusable, cheap to clone (shared `Arc` state) | Yes | Yes | `run` takes `&self`; each run is an independent attempt in its own resource scope. The plan lives until its last clone drops, independently of the `Engine`. |
| `RunRequest` | Owned | Yes | Yes | Concurrent runs pass their own observer; progress events carry no run identity. |
| `Plans` | Owned by one host object, interior lock | Yes | Yes | Keeps one `Plan` per resolved specification and project identity. The lock is held only to read or replace a kept plan, never across a compilation, a run or a callback, so a run started from a callback or another thread proceeds as any other. |
| `Table` | Owned, immutable batches | Yes | Yes | Borrowed views share buffers with the owning table. |
| `ResolvedSpecification` | Owned, immutable | Yes | Yes | No source access after construction. |
| `MemoryResolver` | Owned, interior-mutable | Yes | Yes | Mirrors today's `host_resources::MemoryResolver`. |
| `CancellationToken` | Owned, cheap to clone | Yes | Yes | All clones share one flag. |
| `dyn ResourceResolver`, `dyn ProjectFunctions`, `dyn Progress` | Caller-owned, shared as `Arc` | Required | Required | The facade never calls back into a host while holding an engine lock; the `Plans` lock, too, is held only to read or replace a kept plan. |

Project functions supplied by a host are invoked on the calling thread of the
operation, as today; a host whose runtime is single-threaded (Python, R) owns
that constraint, exactly as the current host crates do.

### Results

`Table` is the facade form of `public_table::PublicTable`: owned Arrow
`RecordBatch`es with their schema, plus `columns()`, `rows()`, `ipc()`,
`stream()` and `preview()`. Stream export follows the ownership rules in
"Arrow host handoff" above unchanged. `preview()` renders the printed preview
(D43, #3030 D167): the header `yamaa.Table: <r> rows x <c> columns`, then
names, types and at most 10 rows of the first 8 columns, so every host prints
a table in the same words. The Arrow type of each public column type is part of the
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
    pub rule: Option<String>,     // the rule card the condition cites
    pub spec_path: Vec<String>,   // ordered path segments
    pub location: Option<Location>,  // where the first spec path is written
    pub message: String,          // rendered from the condition's template
    pub context: DiagnosticContext,
    pub source_span: Option<SourceSpan>,     // byte span in the expression
    pub operand_route: Option<Vec<String>>,  // root-to-node operand steps
}
pub struct SourceSpan { pub start: usize, pub end: usize }  // #[non_exhaustive]
pub struct Report { pub diagnostics: Vec<Diagnostic> }  // #[non_exhaustive]
```

This is the public form of core's `yamaa_core::diagnostic::Diagnostic`, the one
record from the failure site to this edge: the registry names its phase,
condition and rule, and it carries spec paths, structured context and
optional geometry. `DiagnosticContext` is
a structured value map, not a formatted string: `ContextValue` keeps integer
tokens exact (`Int`, `Uint`, or `Wide` with the decimal spelling of a token
outside both 64-bit ranges), fraction/exponent tokens as `Float`, and nested
lists and maps; `to_json()` returns the canonical text the host frames show.
Geometry is optional and absent means unrecorded: a present zero-width span or
empty route is a fact, never a default. `location` stays `None` until issue
locations land (#3262). The one record feeds every form of an issue:
`Report::table()` builds the twelve-column issues table both hosts return
(D28), `text()` its printed line and `to_value()` its JSON object, whose
`context` is an object. The boundary version and record definition in #2674
conform to this struct; the cross-host parity tests are #2677.

Every result prints a `Summary` rendered here (`yamaa::text`, D37), and a
refusal's `Failure::message` is D39's error text: the `Action` that refused,
the first issue's line and the count of the others.

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
`ResolvedPlan::execute` is #2684; partial-result, publication and cleanup rules
on cancellation are #2688. The token shape is settled here so those issues do
not reopen it.

### Features

The facade forwards the existing format features to `yamaa-adapters`:
`yaml`, `parquet`, `readstat` and `odm`, all on by default, and it builds with any
combination of them. Release compatibility smoke-tests no optional formats and
YAML + Parquet alongside the default-format workspace coverage.
With no format feature, CSV and Arrow routes remain, exactly as described in
the feature table above. A capability a build omits fails the same way it does
today: the stable codes (`yaml_reader_unavailable`,
`parquet_reader_unavailable`, `sas_reader_unavailable`,
`odm_reader_unavailable`, `parquet_codec_unavailable`) surface as
diagnostics or as `Error::Unavailable`. The facade adds no native dependencies of its own.

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
- The minimum Rust version is the workspace `rust-version` (1.82 today, the
  release CRAN's two-year-old cargo rule allows on any submission date). CI
  checks and lints the workspace with that release and builds the R source
  package with it, `make quick` fails when a locked dependency needs a newer
  one, and the repository's other builds use the pinned toolchain in
  `rust/rust-toolchain.toml`.
- Rustdoc carries a runnable example for each operation, and one example
  program runs a specification entirely from memory (#2735).

### Distribution

All crates, including the facade, stay `publish = false`. Embedders depend on
the facade by git or path dependency and need no other configuration:

```toml
yamaa = { path = "rust/crates/yamaa", default-features = false, features = ["yaml"] }
```

The `yaml` feature builds the reviewed YAML parser: the vendored
`saphyr-parser` 0.2.1 with its compact-flow fix, which `yamaa-adapters`
depends on by path under its own crate name, `yamaa-saphyr-parser`. A path
dependency reaches the embedder's graph without a `[patch]`, and a graph that
already holds a published `saphyr-parser` keeps that crate beside this one.
Release compatibility's minimum-toolchain job builds the facade from the
outside workspace in `rust/tests/embedder` and runs the parser's compact-flow
controls through it.

Publishing to crates.io is not decided here. It depends on #2583 -- a
published crate cannot depend on the vendored parser by path -- and on the
provenance terms of the vendored ReadStat C sources. Until #2583 lands,
git/path is the distribution route.

### Host-usage mapping

Every adapters or core module the two host crates used before they moved onto
the facade (24 adapters modules and 11 core modules, counted from the host
sources on 2026-10-16, less the removed producer-route modules) maps to a facade
item or is marked internal. The host crates now depend only on the facade and
use none of these modules directly; the facade itself uses 29 adapters modules
and 17 core modules, counted from `crates/yamaa/src` after the producer route
was removed. Any module not listed here is internal by default.

| Module the hosts used | Facade disposition |
| --- | --- |
| `application_measurements` | Internal -- a host-side timing helper; the facade `Report` carries no measurements in the first surface. |
| `arrow_bridge` | Facade -- `Table::stream` and stream import on `Table`; the FFI types themselves are not re-exported. |
| `arrow_table` | Internal -- the engine's table representation; `Table` is the public form. |
| `callback` | Facade -- the `ProjectFunctions` trait and `FunctionError`; `BoundaryError`/`CallbackError` convert into `FunctionError`. |
| `file_application` | Facade -- the one route behind `Engine::check`, `Engine::build`, `Engine::compile`, `Plan::run` and every host operation: a specification alone and each member of a project run prepare, run and report there. |
| `file_define` | Internal -- each section's composition behind `Engine::project_submission`, which `Submission::save` publishes. |
| `file_preparation` | Internal -- `FileSpecification`, the one prepared specification, alone or with its project, behind `Build` and `Plan`. |
| `file_project` | Internal -- project environment capture for preparation. |
| `file_resources` | Internal -- its `Error` converts into `ResolverFailure`/`Error::Internal`. |
| `file_workflow` | Internal -- `SelectionError` converts into diagnostics/`Error`. |
| `host_resources` | Facade -- the `ResourceResolver` trait, `MemoryResolver`, and the resolver request/reply/failure types. |
| `ipc_table` | Internal -- `TableError` converts into the facade `TableError`; `Table::ipc` is the public IPC route. |
| `issue_rows` | Removed -- core's `Diagnostic` is the one record; the facade's `Diagnostic` and `Report` are its public form. |
| `project_application` | Internal -- selection failures convert into diagnostics. |
| `project_attempt` | Internal -- host-failure traversal; failures surface as `Error::Host`. |
| `project_lock` | Facade -- `LockFinding`/`LockReason` are the public lock-finding types, `decode_renv_lock` is the public renv admission, and `verify_uv_lock` compares a uv lock with the installation an `InstalledDistributions` reports (#3030 D172); lock-kind detection stays internal (the engine supplies `LockKind` through `LockContext`). |
| `project_report` | Internal -- the activation and project-function half of the one result builder; `Classified` stays an internal constructor. |
| `project_run` | Removed -- `specification_run::PreparedRun` is the one prepared run, alone or with a project; `Plan` wraps it (#2821), and it stays internal with its `Attempt`. |
| `public_table` | Facade -- `Table`. |
| `resolved_artifact` | Facade -- `ResolvedSpecification` (#2725). |
| `scalar_bytes` | Internal -- scalar decoding is a conversion detail; project functions exchange facade `Scalar` values. |
| `specification_report` | Facade -- the one result builder behind `Build`; its `Identity` folds into `EngineOptions` (`runtime_name`/`runtime_version`); the module is internal. |
| `style_values` | Internal -- the value comparison that proves `lint`'s fixes; `style_same_value` exposes it for embedders. |
| `yaml_decode` | Internal -- `DecodeFailure` converts into diagnostics/`Error::Unavailable`. |
| `yamaa_core::application_observer` | Facade -- the `Progress` trait with facade `Phase`/`Outcome` enums; the `Scope` guard stays internal. |
| `yamaa_core::domain_entry` | Internal -- the observed preparation and build phases behind `Engine`. |
| `yamaa_core::function_batch` | Facade -- `ProjectFunctions` batch invocation with facade scalar arguments; `BatchFunctionPort` stays internal. |
| `yamaa_core::function_invocation` | Facade -- `FunctionError` is its public form; `HostError` stays internal. |
| `yamaa_core::function_signature` | Facade -- `FunctionIdentity` and `Signature`/`Parameter` are the public activation types, and `ColumnType` is the declared result type; `LogicalSignature` stays internal. |
| `yamaa_core::project_activation::ActivationPort` | Facade -- the `ProjectFunctions` trait; the core port stays internal. |
| `yamaa_core::project_environment` | Facade -- `LockKind` and `LockContext`; environment admission stays internal. |
| `yamaa_core::project_function` | Facade -- `HostLanguage` and the facade `Scalar` value type; the core `Language` enum stays internal. |
| `yamaa_core::table::ValueRef` | Facade -- `ScalarRef`; the core table stays internal. |
| `yamaa_core::temporal` | Facade -- `Date` and `DateTime`, with civil fields and epoch day or second counts; collected precision is dropped at the boundary. |
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
(`rust/crates/yamaa-core/src/table_grouping.rs`) and the window runs of
`rust/crates/yamaa-core/src/dataset/columnar/windows.rs` means a **key
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
  candidate row within $P_i$) using only the immutable `ResolvedPlan` and
  immutable secondary tables. Concatenating the per-partition outputs in
  partition order reproduces the whole-table output directly.
- **`safe-with-combine-step`**: Evaluates per partition $P_i$ while updating a
  bounded, named per-dataset accumulator (handler counts, key-group state, or
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

Key files: `rust/crates/yamaa-core/src/dataset/columnar/mod.rs`, `eval.rs`,
`keygrain.rs`, `relations.rs`, `joins.rs`, `functions.rs`, `scalar.rs`,
`predicates.rs`, `odm.rs`; `rust/crates/yamaa-core/src/table_grouping.rs`.

| Operation / construct | Location | Classification | Partition state and notes |
| --- | --- | --- | --- |
| `RowMode::Records` (primary driver) | `columnar/mod.rs` (`process_template`) | `safe` (or `safe-with-combine-step` with handlers) | Each row $j$ in $P_i$ forms a single-member candidate `[base_i + j]`. Pre-window row-local filters run per row; the row's selection identity (`Identity::Candidate` in `columnar/frame.rs`) advances monotonically across partitions. |
| `RowMode::Keys` (`keygrain::construct`) | `columnar/keygrain.rs`, `columnar/group.rs` | `safe-with-combine-step` (key and collect plans) / `global` (unprojected random primary reads) | Source filter and key assignments run per row. Distinct non-missing key tuples accumulate in first-occurrence order across partitions; each missing-key record remains an independent candidate at its global row position so `OutputKeys` reports it. `Collect` folds per key group across partitions. Plans that later index arbitrary unprojected primary rows via `candidate.members` require whole-table state (`global`). |
| `RowMode::Groups(keys)` | `columnar/mod.rs`, `table_grouping.rs` | `global` (arbitrary partitions) / `safe-with-combine-step` (group-accumulator fold or key-disjoint partitions) | On arbitrary partitions, a key group in `table_grouping::partition` can span multiple $P_i$, and template assignments/post-window filters read `candidate.members`. Safe per partition when inputs are co-partitioned by `keys`, or combinable when all member reads reduce through per-group accumulators. |
| `RowMode::Driver { source, mode }` | `columnar/mod.rs` | `global` (run-once secondary driver) | Drives candidates from `secondary[source - 1]`, not the primary table. Must execute once per dataset run rather than repeating per primary partition. |
| `RowMode::IntermediateDriver { index, source, mode }` | `columnar/mod.rs`, `columnar/relations.rs` | `global` | Consumes `relations::driver` over the full donor relation; runs once per dataset when backed by a secondary source and requires full primary/`SELF` state otherwise. |
| `Expression::Literal`, `Expression::Column` | `columnar/eval.rs` | `safe` | Pure candidate-local reads with no source or cross-row state. |
| `Expression::Source(column)` | `columnar/eval.rs` | `safe` | Reads `candidate.members[0]` in the active partition; cell errors translate `source_row` to `base_i + local_row`. |
| `Expression::SourceBinding` | `columnar/eval.rs` | `safe-with-combine-step` | Row evaluation delegates to `source` or returns the `absent` literal; the handler counts (`absent`, `multiple_matches`) combine across partitions by checked addition. |
| `Expression::FirstAvailable` | `columnar/eval.rs` | `safe` (row-local operands) / `safe-with-combine-step` (with `Collect` or handlers) | Evaluates operands in authored order per candidate; combines handler counts and any per-key `SelectionRead::Collect` state. |
| Pure scalar and string/temporal expressions (`Compute`, `Round`, `Extreme`, `StrTemplate`, `StrExtract`, `StrCase`, `StrPad`, `Imputation`, `Temporal`, `Flag`, `Case`) | `columnar/scalar.rs`, `predicates.rs`, `eval.rs` | `safe` (pure) / `safe-with-combine-step` (with recovery handlers or `Collect`) | Value computation is row-local when bindings are `Read::Column` or `Read::Source`. Recovery counts (`Missing`, `Invalid`, `Unconvertible`) in the handler counts combine by checked sum in registration order. |
| `Expression::SourceLookup` | `columnar/joins.rs` | `safe` (no selection handler) / `safe-with-combine-step` (with `selection`) | Matches candidate keys against the immutable whole-table `secondary[lookup.source]`. When `selection` resolves `count > 1`, `HandlerKind::MultipleMatches` combines in the handler counts. |
| `Expression::Joined`, `Expression::JoinedSource` | `columnar/joins.rs` | `safe-with-combine-step` (`Secondary`) / `global` (`Primary`) | Secondary joins scan immutable `secondary[index]` and combine `MultipleMatches` handler counts. Primary self-joins (`AggregateSource::Primary`) scan all primary rows and are `global`. |
| `Expression::Collect` | `columnar/scalar.rs` | `safe` (`RowMode::Records`) / `safe-with-combine-step` (grouped/key-grain) | In `Records` mode `candidate.members` has length 1. Across partitions in `Keys`/`Groups` mode, `Collect` merges distinct present `Key` sets plus the best `(order_by, global_source_row)` extremum and `MultipleMatches` handler count. |
| `Expression::ProjectFunction` | `columnar/functions.rs`, `function_batch.rs` | `safe` (row-local inputs) / `safe-with-combine-step` (with `FunctionInput::Collect`) | `functions::admit` runs once at `Admission`. Invocations run in deterministic global candidate order with failure `RowIdentity.position = out_base_i + local_row`. |
| `Expression::Odm` | `columnar/odm.rs` | `global` (arbitrary partitions) / `safe-with-combine-step` (key-scoped or hierarchy-partitioned) | `odm::odm` scans the source (or `candidate.members` under `OdmScope::Keys`) to enforce strict item uniqueness (`OdmFinding::NotUnique`). On arbitrary partitions, non-key scopes are `global`; under `OdmScope::Keys` or hierarchy-co-partitioned inputs, selected values and `IDENTIFYING` sets combine per scope key. |

#### 2. Mapping and cut

Key files: `rust/crates/yamaa-core/src/mapping_cut.rs`,
`rust/crates/yamaa-core/src/dataset/columnar/scalar.rs`.

| Operation / construct | Location | Classification | Partition state and notes |
| --- | --- | --- | --- |
| `Mapping::new`, `Cut::new`, `Cut::declaration_findings` | `mapping_cut.rs` | `safe` | Plan-admission validation (`MappingAmbiguousDictionary`, `CutDeclaration`); runs once before data access. |
| `Expression::MappingCut` (`Mapping::select`, `Cut::select`) | `mapping_cut.rs`, `columnar/scalar.rs` | `safe-with-combine-step` | Dictionary matching and `breaks.partition_point` interval selection are pure functions of a single cell. Reached `missing` and `unmapped` handlers count in the handler counts, which combine across partitions by checked addition. Fatal diagnostics (`MappingMissing`, `MappingUnmapped`, `MappingInput`, `CutMissing`, `CutInput`, and unconvertible handler literals) attach `RowIdentity` at global position `out_base_i + local_row`. |

#### 3. Intermediates

Key files: `rust/crates/yamaa-core/src/dataset/columnar/relations.rs`,
`index.rs`, `donor.rs`.

| Operation / construct | Location | Classification | Partition state and notes |
| --- | --- | --- | --- |
| `IntermediateSource::Input(index)` (uncorrelated filter, `unique`, key index, uncorrelated `derivations`) | `columnar/relations.rs`, `index.rs`, `donor.rs` | `safe-with-combine-step` | Depends only on immutable `secondary[index]`. `relations::verify` runs once before primary partitions; the narrowed records, the key `Index` and the uncorrelated materialized donor are computed lazily once and shared across all partitions. Per-candidate choices and the handler counts (`NoMatch`, `MultipleMatches`, nested `Unconvertible`) combine across partitions. |
| `IntermediateSource::Input(index)` (correlated `filter`, `range`, or correlated `derivations` where `!derived.current.is_empty()`) | `columnar/relations.rs`, `donor.rs` | `safe-with-combine-step` | Evaluates current-row operands per candidate and probes/materializes against the full immutable `secondary[index]`. Cache entries keyed by the row's selection identity are candidate-local and dropped at partition end; handler counts combine across partitions. |
| `IntermediateSource::Primary` (all reads, `driver`, `materialize`, and `unique` verification) | `columnar/relations.rs`, `donor.rs` | `global` | Scans `0..primary.row_count()` to check uniqueness, evaluate eligibility, count matches, and break ties by global donor row index. Cannot execute on a single primary partition without whole-table primary state. |
| `IntermediateSource::SelfRows` (`SELF` donors, `State::publish_self`, `Step::VerifyIntermediates { self_rows: true }`) | `columnar/relations.rs`, `mod.rs` | `global` | The donor relation is the completed rows of prior `RowTemplate`s, published by `State::publish_self`, and the self-rows verification partitions all `SELF` rows to enforce `IntermediateUnique`. Requires all prior templates' global candidates. |
| Windowed intermediate match keys (`MatchValue::Expression` with `expression.windowed`, `State::begin_columns`) | `columnar/relations.rs` | `global` | Key values depend on a window over the constructed relation; `begin_columns` invalidates template-phase caches because column-phase windows observe the complete output relation. |
| `relations::check_unobserved_filter_types` | `columnar/relations.rs` | `safe` | Inspects static deferred filter-type findings on `plan.intermediates()` after column completion; independent of partition count. |

#### 4. Aggregates

Key files: `rust/crates/yamaa-core/src/dataset/columnar/aggregates.rs`,
`rust/crates/yamaa-core/src/aggregate_reduction.rs`.

| Operation / construct | Location | Classification | Partition state and notes |
| --- | --- | --- | --- |
| `AggregateRelation::CurrentGroup` (within single-partition candidates, e.g., `RowMode::Records` or key-disjoint partitions) | `columnar/aggregates.rs` | `safe` (or `safe-with-combine-step` with `derive_records` handlers) | All `candidate.members` reside in the active partition $P_i$; `select`, `derive_records`, and `reduce_selected` complete locally and merge the handler counts. |
| `AggregateRelation::CurrentGroup` (groups split across arbitrary partitions) | `columnar/aggregates.rs` | `safe-with-combine-step` (pure reducers) / `global` (correlated primary/output derivations or non-reduced representative reads) | Reducers (`Count`, `Sum`, `Mean`, `Min`, `Max`, `SingleValue`) and input-type checks combine across partitions per group; plans whose `AggregateResult::Numeric` reads an unreduced donor column from `representative` or whose `derive_records` performs global lookups require full group materialization (`global`). |
| `AggregateRelation::Matched { source: AggregateSource::Secondary(index), .. }` | `columnar/aggregates.rs` | `safe-with-combine-step` | Matches each candidate against immutable `secondary[index]`, runs `derive_records` and `reduce_selected` locally, and combines the handler counts. |
| `AggregateRelation::Matched { source: AggregateSource::Primary, .. }` | `columnar/aggregates.rs` | `global` | Scans `0..primary.row_count()` for every candidate row to select matching primary donors and reduce them. |
| `AggregateRelation::Output { group_by }` | `columnar/aggregates.rs` | `global` | Groups all constructed output candidates by `group_by` once per unit and reads the current row's group. |

#### 5. Windows and output ordering

Key files: `rust/crates/yamaa-core/src/dataset/columnar/windows.rs`,
`checks.rs`.

| Operation / construct | Location | Classification | Partition state and notes |
| --- | --- | --- | --- |
| `Expression::Window`, `Expression::ContextWindow` (`WindowKind::BaselineFlag`, `RowValue`, `Locf`, `RowNumber`, `Competition`, `Dense`) | `columnar/windows.rs` | `global` (arbitrary partitions) / `safe-with-combine-step` (only when input partitions are disjoint on non-empty `window.group_by` and preserve within-group order) | `windows::prepare` partitions the entire candidate relation by `window.group_by` (or places all rows into one group when `group_by` is empty), sorts members by `window.order_by` with global candidate index tie-breaking (`left.cmp(&right)`), evaluates `window.filter` in sorted order, and computes ranks, row-offset lookups, carry-forward values, or `BaselineFlag` / `BaselineAmbiguity`. Over arbitrary partitions, all window kinds are `global`. |
| `checks::output_order` (`DatasetPlan::output_order`) | `columnar/checks.rs` | `global` (or `safe-with-combine-step` via global k-way merge after verification) | Runs after `OutputKeys` and `Verification` succeed. Sorts accepted rows by `output_order` terms using pre-sort global candidate index `left.cmp(&right)` as the final tie-breaker. |

#### 6. Output key checks and verification `Check` variants

Key files: `rust/crates/yamaa-core/src/dataset/mod.rs`,
`rust/crates/yamaa-core/src/dataset/columnar/checks.rs`,
`rust/crates/yamaa-core/src/dataset_checks.rs`.

| Check / variant | Phase & location | Classification | Combine state and notes |
| --- | --- | --- | --- |
| `OutputMissingKeyFailed` (`keys[position]` completeness) | `OutputKeys` (`columnar/checks.rs`) | `safe-with-combine-step` | Per key column, each partition records offending global row identities (`out_base_i + local_row`); combine sums `evaluated_count` and `failed_count`, sets `output_rows = N`, and concatenates `offending_rows`. |
| `OutputDuplicateKeyFailed` (`keys` uniqueness) | `OutputKeys` (`columnar/checks.rs`) | `safe-with-combine-step` | Accumulates distinct key tuples across partitions with first-seen group order, first member `RowIdentity`, and member count. At finalize, `evaluated_count` is total distinct key groups, `failed_count` is groups with count $> 1$, and `offending_rows` lists `members[0]` for each duplicate group in first-seen group order. |
| `IntermediateUnique` (`relations::verify`) | `Derivation` (`columnar/relations.rs`) | `safe-with-combine-step` (`Input`) / `global` (`Primary`, `SelfRows`) | Secondary-source `unique` checks run once against immutable `secondary[index]`. Primary and `SELF` `unique` checks require the global donor table. |
| `Check::NotMissing` | Column / `Verification` (`columnar/checks.rs`, `dataset_checks.rs`) | `safe-with-combine-step` | Row-local `matches!(value, Value::Missing)`; combine sums `evaluated_count` and `failed_count`, sets `output_rows = N`, and concatenates `offending_rows` in global row order. |
| `Check::Mandatory(scope)` | Column (`columnar/checks.rs`, `dataset_checks.rs`) | `safe-with-combine-step` | As `NotMissing`, over the rows the optional `scope` selects by where clauses. |
| `Check::DataType { form, scope }` | Column (`columnar/checks.rs`, `dataset_checks.rs`) | `safe-with-combine-step` | Row-local ISO 8601 form test of each present value over the rows the optional `scope` selects; `CheckRecord` combines across partitions. |
| `Check::AllowedValues(values)` | Column (`columnar/checks.rs`, `dataset_checks.rs`) | `safe-with-combine-step` | Row-local membership test against fixed `values`; `CheckRecord` combines by addition/concatenation across partitions. |
| `Check::Codelist { id, values }` | Column (`columnar/checks.rs`, `dataset_checks.rs`) | `safe-with-combine-step` | Row-local `compare_present` test against `values`; combines `CheckRecord` counts, `offending_rows`, and `CodelistObservation { id, values }`. |
| `Check::ValueCodelists { overrides, bindings }` | Column (`columnar/checks.rs`, `dataset_checks.rs`) | `safe-with-combine-step` | Runs once the target column and every where column its clauses read are complete on the candidate. Emits one `CheckRecord` per binding in `bindings` order; each binding's counts, `offending_rows`, and `CodelistObservation.values` combine across partitions. |
| `Check::Inventory { values, scope }` | Column (`columnar/checks.rs`, `dataset_checks.rs`) | `safe-with-combine-step` | Row-local check gated by the optional `scope`'s where clauses on the same candidate; combines counts, `offending_rows`, and `inventory: Some(Vec<Value>)`. |
| `Check::ValueMaxLength { max, scope }` | Column (`columnar/checks.rs`, `dataset_checks.rs`) | `safe-with-combine-step` | Row-local `text.chars().count()` bound check over the rows `scope` selects by where clauses; `CheckRecord` combines across partitions. |
| `Check::ValueEntries(entries)` | Column (`columnar/checks.rs`) | `safe-with-combine-step` | Row-local count of the entries whose where clause selects the candidate; a row two entries select offends, and `CheckRecord.entries` keeps the matched ids aligned with `offending_rows` across partitions. |
| `Check::ValueEntryMatch { id, clause }` | Column (`columnar/checks.rs`) | `safe-with-combine-step` | The artifact is the one evaluated unit: the entry matches when any partition holds a row `clause` selects. |
| `Check::Range { min, max }` | Column (`columnar/checks.rs`, `dataset_checks.rs`) | `safe-with-combine-step` | Row-local numeric range comparison against `min`/`max`; `CheckRecord` combines across partitions. |
| `Check::MaxLength(usize)` | Column (`columnar/checks.rs`, `dataset_checks.rs`) | `safe-with-combine-step` | Row-local Unicode scalar count check `text.chars().count() > max`; `CheckRecord` combines across partitions. |
| `Check::WideMaxLength(RowCountBound)` | Column (`columnar/checks.rs`, `dataset_checks.rs`) | `safe-with-combine-step` | Row-local `max.compare_count(text.chars().count()) == Ordering::Less`; `CheckRecord` combines across partitions. |
| `Check::Matches(Pattern)` | Column (`columnar/checks.rs`, `dataset_checks.rs`) | `safe-with-combine-step` | Row-local linear-time regex search with no step budget; `CheckRecord` combines across partitions. |
| `Check::InvalidDiagnostic(Diagnostic)` | Column / `Verification` (`columnar/checks.rs`) | `safe` | Data-independent deferred diagnostic; fires at its declaration checkpoint with the prefix `records` accumulated before that checkpoint. |
| `Check::InvalidDeclaration { code, reason }` | Column / `Verification` (`columnar/checks.rs`) | `safe` | Data-independent deferred compiler finding; fires at its declaration checkpoint with the prefix `records` accumulated before that checkpoint. |
| `Check::PredicateDeclaration(BoundPredicate)` | `Verification` (`columnar/checks.rs`) | `safe` | Schema-only sample validation (`validate_predicate`); reads no candidate rows and emits no `CheckRecord`. |
| `Check::Assert { when, require }` | `Verification` (`columnar/checks.rs`) | `safe-with-combine-step` (column / secondary-intermediate reads) / `global` (primary or `SELF` intermediate reads) | `validate_predicate` runs once on synthetic schema samples; each candidate evaluates `when` and `require` without short-circuiting. Counts and `offending_rows` combine across partitions unless a bound intermediate is `global`. |
| `Check::AllOrNone(columns)` | `Verification` (`columnar/checks.rs`) | `safe-with-combine-step` | Row-local check that all `columns` share the same missingness on each candidate; `CheckRecord` counts and `offending_rows` combine across partitions. |
| `Check::Unique(columns)` | `Verification` (`columnar/checks.rs`) | `safe-with-combine-step` | Tracks distinct column-tuple keys across partitions with first-seen group order and all member `RowIdentity`s per group. At finalize, `evaluated_count` is total distinct groups, `failed_count` is groups with $> 1$ member, and `offending_rows` flattens repeated groups in first-seen group order, then within-group global row order. |
| `Check::RowCount { min, max }` | `Verification` (`columnar/checks.rs`) | `safe-with-combine-step` | Sums output row counts $N = \sum_i \lvert P_i^{\text{out}} \rvert$ across partitions and evaluates `row_count_accepts(N)` once at finalize (`evaluated_count = 1`, `output_rows = N`). |
| `Check::WideRowCount { min, max }` | `Verification` (`columnar/checks.rs`) | `safe-with-combine-step` | Sums output row counts $N = \sum_i \lvert P_i^{\text{out}} \rvert$ across partitions and compares against `RowCountBound` once at finalize (`evaluated_count = 1`, `output_rows = N`). |
| `Check::ScopedRowCount(RowCount)` | `Verification` (`columnar/checks.rs`) | `safe-with-combine-step` (column / secondary-intermediate predicates) / `global` (primary or `SELF` intermediate predicates) | Evaluates `filter` (`admitted`) and `when` (`bound`) per row and folds into per-`group_by` tuple accumulators (or one global group when `group_by` is `None`) storing `(first_seen_order, first_row_identity, total_members, admitted_count, any_bound)`. Filtered-empty groups remain counted in `evaluated_count` unless exempted by `when.is_some() && !any_bound`. Finalize checks `accepts(admitted_count, total_members)` in first-seen group order and populates `offending_rows`, `counts`, and optional `denominators`. |

### Whole-table equivalence specification

Let $T$ be a primary source table with schema $S$ and $R$ rows, and let $P_0,
P_1, \dots, P_{k-1}$ ($k \ge 1$) be any partition of $T$ into tables with schema
$S$ and row counts $r_0, r_1, \dots, r_{k-1}$ such that $\sum_{i=0}^{k-1} r_i =
R$ and $P_i.\text{cell}(j, c) = T.\text{cell}(\text{base}_i + j, c)$ for all $i
\in [0, k)$, $j \in [0, r_i)$, and columns $c$, where $\text{base}_i = \sum_{m <
i} r_m$. Secondary tables and the function port are identical in both runs.

Partitioned execution over $(P_0, \dots, P_{k-1})$ satisfies **whole-table
equivalence** with `ResolvedPlan::execute(T, secondary, functions, observer)` if
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
     (`relations::select`), window sorting (`columnar/windows.rs`), and
     presentation sorting (`checks::output_order`)
     compares global source row indices $\text{base}_i + j$ or global pre-sort
     candidate positions $\text{out\_base}_i + r$, never partition-local offsets.
   - **Presentation order**: When `output_order` is non-empty, `checks::output_order`
     runs after `OutputKeys` and `Verification` succeed and produces the exact
     stable merge-sort permutation of the whole-table candidate sequence.

3. **Diagnostic and error identity**:
   - **Phase and checkpoint precedence**: Execution preserves the whole-table
     phase order (`Admission` -> `Derivation` [eager secondary/primary
     intermediate `verify` -> template candidate construction -> column-by-column
     completion with interleaved `check_columns` prefix checkpoints ->
     `check_unobserved_filter_types`] -> `OutputKeys` -> `Verification` ->
     `checks::output_order` -> `Finished`). If whole-table execution fails,
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

4. **Retained observations (`handler_counts`, `template_rows` and `verifications`)**:
   - `ExecutionAttempt.handler_counts`: Handler sites are registered in
     deterministic plan order during `Admission`; per-site counts sum across
     partitions via checked addition in `handlers::Handlers` (raising
     `ExecutionError::HandlerAccounting` on overflow) and match the whole-table
     snapshot on both success and failure at the same checkpoint.
   - `ExecutionAttempt.template_rows`: Each completed row template's emitted
     rows and driver records read, in construction order, feed the run
     report's `rows` rows. Partition counts sum per template, and a failed
     attempt retains exactly the templates completed before the failure.
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

1. **Attempt driver seam (`rust/crates/yamaa-core/src/dataset/columnar/mod.rs`)**:
   - `ResolvedPlan::execute` runs `execute_inner` and `execute_admitted`. Each
     step of the schedule carries its `partition` class, its `effects` and a
     `checkpoint` flag (`StepInfo` in `dataset/resolved/mod.rs`). Split the run
     into three explicit stages:
     1. *Run admission*: `functions::admit`, secondary-source schema and
        `Limits::table` checks, handler site registration (`handlers::Handlers`
        registers every site of the resolved handler table at zero), and the
        one-time `relations::verify` for `IntermediateSource::Input`.
     2. *Per-partition step*: Accept each primary partition $P_i$ with
        `(base_source_row, base_output_row)`, update cumulative `Limits::table`
        counters, construct partition candidates, and evaluate row-local /
        combinable column assignments. Poll the cancellation token (#2684) at
        steps marked `checkpoint`, between partitions, and between
        intermediates.
     3. *Combine and finalize*: Finalize cross-partition group/key state, run
        `relations::check_unobserved_filter_types`, combine `OutputKeys` and
        `Verification` accumulators against total `output_rows = N`, and run
        `checks::output_order`.

2. **Grouping and key-grain seam (`rust/crates/yamaa-core/src/table_grouping.rs`, `rust/crates/yamaa-core/src/dataset/columnar/keygrain.rs`)**:
   - `table_grouping::Key<'a>` borrows `&'a str`
     from a single table snapshot. Cross-partition grouping introduces an owned
     counterpart (`OwnedKey` / canonical `Vec<Value>` with `-0.0` canonicalized
     to `0` bits and temporal precision ignored in `Ord`/`Eq`, matching `Key<'a>`
     semantics) so group maps outlive individual partition borrows by direct
     value comparison.
   - In `keygrain::construct`, replace the retained whole-table driver records
     of each candidate row (`Frame` members) with a per-key combine state
     holding:
     first global source row index, per-column `BTreeSet<OwnedKey>` of distinct
     present values plus best `(order_by_values, global_source_row)` for
     `Collect`, and running `(count, NumericReducer)` state for `Count` and
     `Reduce`.

3. **Intermediate cache lifecycle seam (`rust/crates/yamaa-core/src/dataset/columnar/relations.rs`, `index.rs`, `donor.rs`)**:
   - Separate `relations::State` into two lifetimes:
     - *Run-scoped state* (retained across all primary partitions): each
       relation's narrowed records, key `Index` and uncorrelated materialized
       donor for `IntermediateSource::Input(index)`.
     - *Partition-scoped state* (cleared after each partition $P_i$ finishes its
       column pass): choices and correlated materialized donors, both keyed by
       the row's selection identity. Clearing partition-scoped entries at each
       partition boundary bounds intermediate memory to the active partition and
       static secondary tables.

4. **Column-verification checkpoint seam (`rust/crates/yamaa-core/src/dataset/columnar/mod.rs`, `checks.rs`)**:
   - `complete_columns` interleaves column derivation with `check_columns`
     prefix checkpoints so an error-severity column check at column $c$ preempts
     derivation failures in column $c + 1$.
   - In partitioned execution, per-column `CheckRecord` accumulators collect
     `(evaluated_count, failed_count, offending_rows, codelist, inventory)` per
     column checkpoint across partitions, while any derivation failure in a
     later column $c' > c$ is ordered after earlier column-$c$ check failures so
     whole-table failure precedence is preserved.

5. **Dataset verification combine seam (`rust/crates/yamaa-core/src/dataset/columnar/checks.rs`)**:
   - Factor `verify_output` and its predicate and row-count checks behind a
     per-partition accumulator that ingests `(out_base_i, rows)` per partition
     and finalizes once with total `output_rows = N`:
     - Row-local checks (`OutputMissingKeyFailed`, `NotMissing`, `AllowedValues`,
       `Codelist`, `ValueCodelists`, `Inventory`, `ValueMaxLength`, `Range`,
       `MaxLength`, `WideMaxLength`, `Matches`, `Assert`, `AllOrNone`) append
       global `RowIdentity`s and sum counts.
     - Key-grouped checks (`OutputDuplicateKeyFailed`, `Check::Unique`,
       `Check::ScopedRowCount`) fold into `BTreeMap<Vec<OwnedKey>, GroupState>`
       preserving first-seen group order.
     - Whole-table count checks (`Check::RowCount`, `Check::WideRowCount`)
       evaluate `row_count_accepts(N)` at finalize.

