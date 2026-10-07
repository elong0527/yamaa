# Native application ports

The #1755 application foundation moves the original-document
capture/decode/bind/execute lifecycle into `yamaa-engine::specification_run`
and output gates into `yamaa-engine::specification_output`.
Python and R use that service through the existing bounded facade. Its resource
and codec interfaces are native traits; adapters translate values and errors.

| Port | Owner | Current implementations | Failures |
| --- | --- | --- | --- |
| `specification_run::SourcePort` | engine | Python and R capture bridges; engine and adapter test fakes | Original opaque capture error; regressing snapshot counter |
| `specification_run::SourceDecoder` | engine | Adapter CSV/Parquet/Arrow decoder; engine test fake | Original decoder error; byte/cell capacity limits |
| `specification_output::ArtifactEncoder` | engine | Adapter CSV/Parquet encoders; native test fake | Original codec error; checked output byte limit |
| `specification_output::OutputReport` | engine | Portable JSON report formatter; native test fake | Observation or report-budget failure before publication |
| `specification_output::ArtifactPort` | engine | Python/R atomic publication bridges; native test fake | Original opaque publication error, without retry |
| `TableAccess` | core | Immutable Arrow snapshots and engine output tables; test fakes | Bounds or original opaque cell-access error |
| `inheritance::SourcePort` | engine | Existing inheritance bridge | Source failure or traversal resource limit |
| `function_invocation::FunctionPort` and `dataset::FunctionBindings` | engine | Python/R native callback bridges | Original host exception, rejected representation or typed invocation failure |

## Current lifecycle

The bounded `yamaa-engine::domain` use case now owns executable preflight.
`check` consumes an already admitted normalized model and returns an immutable
`CheckedSpecification`; it has no study-data, function or publication port.
`build` performs that same check before invoking source authority. Both installed
host adapters retain the checked capability and call its `build_into` operation
inside their existing panic boundary, so partial source observations survive.
Native fake-port tests pin unsupported-before-capture behavior, cached repeated
builds, opaque error identity and incomplete attempts after a host panic.

This internal check is vocabulary admission, not the complete public `check`
contract in #1751. Binding findings that depend on source schemas, and existing
deferred formula diagnostics, still occur after ingestion. Raw file capture,
bundled schema selection, environment/lock/function-test gates and host issue
tables are not provided by this use case yet. It does not expose resolved YAML
or choose either host's public result representation.

The internal `_prepare_document` (Python) and `.prepare_document` (R) boundaries
now accept raw entry bytes and parent/path ports without caller-supplied schema
modules. Shared decoding selects standalone or inherited preparation by field
presence, including an invalid null `parents` field; there is no retry/fallback.
Entry bytes are decoded once and retained with the chosen lifecycle's provenance.
The package embeds the current domain schema closure, reproduced byte-for-byte by
`generate_shipped_schema.py` from authoritative YAML. Shared admission checks
the declared `schema_version` and rejects unavailable versions before parent IO.
Environment and submission schema roots are separate and are not included yet.
Installed original-document tests use this boundary for all six documents and
the seven complete inherited-loader failures. Public path APIs and complete
capture diagnostics are still required before conformance inventory promotion.

Core preparation admits the supported vocabulary before a source request can be
made. An engine attempt requests immutable byte snapshots in input declaration
order, measures each actual snapshot counter, and decodes owned tables. After
complete ingestion it binds all actual schemas, checks total cell capacity and
executes the admitted core plan with its selected driver. Retained snapshots,
including invalid inputs, share one cumulative byte budget. The same prepared model can be
reused; each attempt repeats the request and execution. A cached capture still
counts as a request, with zero newly created snapshots. Held bytes are compared
directly by the capture implementation; there is no digest identity.

Capture, decoding, binding, resource and runtime failures remain distinct.
A decoder explicitly classifies semantic ingestion findings that can be collected
while later declared sources are captured. Resource and opaque boundary failures
stop the collection. Partial tables remain held internally, but portable table
observations appear only after the whole input collection was ingested.
No failed operation retries or falls back to another implementation. The attempt
retains the exact captured bytes and decoded table for reporting, including after
binding or runtime failure. `execute_with_port_into` updates these observations
in place so an outer panic boundary can preserve completed observations without
reopening or reparsing input. An interrupted attempt remains explicitly
incomplete until the adapter maps its boundary failure.

The CSV adapter retains its existing profile, type conversion and capacity
settings. The held Parquet decoder uses the compiler's declared profile and
empty-string policy, with bounded physical decoding and ordered semantic
findings described in [PARQUET_SOURCE.md](PARQUET_SOURCE.md).
The adapter serializes the engine result through the existing response
formatter; it no longer chooses capture, binding or execution order. Tests use
native fake ports to pin order, cached counts, original non-Clone errors,
capacity failures before cell reads, and observations surviving a panic in a
later source. The portable report preserves every requested capture and the
engine's actual handler counts, including inherited named-selection handlers.

## Output lifecycle

Building can now stop before publication and return an owned result.
`specification_output::prepare` checks the output declaration, resolves the
projection, bounds encoding and prepares observations without accepting any
publication authority. Its `PreparedOutput` retains the artifact path, exact
bytes and projection. A failed or rejected result has no artifact and cannot
call a publisher through `save`. A successful save borrows the held bytes;
repeated explicit saves repeat only publication, including after a publisher
failure. They never read data, evaluate expressions or encode the output again.

The adapter retains the admitted output projection as owned Arrow IPC and keeps
prospective artifact observations private. Build-phase observations exclude
artifacts; `save` returns publication observations only after its callback
succeeds. Later saves do not modify the original build-phase observations.
Python exposes this through the internal prepared specification's `build` method
and an owned result; R uses registered result handles. Neither binding retains
the capture callback or borrows a source/result buffer. Installed tests compare
all six complete reports and exact saved bytes, and check failed-save rejection,
opaque publication errors/interruptions, repeated saves and expired R handles.
These internal results supply the build/save boundary for the public API;
host data-frame/issue/log properties and file-path entry points remain to connect.

Successful response preparation retains the engine's accepted typed execution.
Publication does not infer acceptance from serialized JSON or decode its own IPC
output. An unsuccessful attempt reaches only the failure-report formatter.
For an accepted execution the engine orders initial observations, core output
findings, projection resolution, bounded encoding, complete bounded report
preparation, and one publication request. An error at any step stops later
requests. The engine independently rejects a codec result beyond the supplied
byte limit, even if the codec violated its allocation contract.

Report formatting and CSV/Parquet representation remain adapter responsibilities. A
report error, including its size budget, must be returned before publication.
Opaque codec/report/publication failures are retained without cloning or retry.
The current report still uses the existing portable transport for diagnostic
observations; replacing that formatting transport is a later cleanup. Native
fake ports pin declaration order, rejection, codec overruns, error identity,
report-before-publication order and repeated explicit requests.

The Parquet output adapter writes the closed profile directly from the admitted
typed table: optional UTF-8 strings, signed INT64, DOUBLE, date INT32 and
timezone-free microsecond timestamps. It preserves column projection, record
order, missing values, empty strings, full-range integers and finite float bits.
Files are uncompressed and have no key/value metadata. The private build/save
path selects the encoder from the declared output extension; input Parquet is
still explicitly Unsupported before study-data capture.

Resource policy limits projected columns/cells, schema construction, per-column
staging in groups of at most 1,024 rows, and encoded output bytes. Charges precede
text copies; a codec failure or refusal discards the incomplete file. The output
sink limit alone is not a bound on total process memory. Portable artifact
observations record actual byte length and logical JSON records, with complete
stored calendar fields and exact finite scalar spelling. The binary bytes are
retained for explicit save, never reconstructed from those report records.

Rust tests exercise all closed types, boundary values, row-group order, empty
tables, projection, error identity and limits. Installed tests vary only the
ordered-sum case's output container and preserve its independently committed
non-artifact observations. Python blocks semantic imports during native build
and save, then uses PyArrow as an independent reader; a separate hand-authored
case checks every closed type and exact boundary value. R builds and saves with
Python unavailable. These checks do not promote the six-case inventory from its
reference-assisted level or qualify the unimplemented public file frontend.

## Remaining migration

This is an exercised source-read foundation, not completion of #1755. The
unified resource reader still needs specification/layer/producer/environment and
codelist resources, with resolved paths and declaring-file provenance. Current
source bridges retain their existing path behavior; the #1751 target resolves
each path relative to its declaring file without an approved-root boundary.

Parquet decoding, general publication/save policy, environment
activation, shared reusable test fakes, and consolidation of the two function
interfaces remain open. The existing inheritance JSON callback bridge also
remains to be replaced. No new unimplemented port is presented as a working
capability, and no public default, golden fixture or qualification level changes.

### Captured standalone schema findings

Standalone schema validation and named-window expansion retain semantic findings
with the exact captured entry, the admitted schema, and the input arena for the
failed pass. The arena for window expansion is normalized; it must not be
replaced by the raw document when resolving context occurrence IDs. Final model
findings carry text context and do not need an arena copy. Successful preparation
does not make these failure snapshots.

The shared adapter resolves context references before either host receives them:
version values, permitted values, patterns, minimum lengths and sizes become the
same portable validation diagnostic records as runtime findings. Mathematical
integer context is serialized exactly, including beyond signed i64. This uses
serde_json's arbitrary-precision number representation only at the transport
boundary; it does not widen runtime integers or change arithmetic policy.

Context expansion charges a conservative escaped-JSON byte budget before copies.
A resource refusal or invalid internal context remains a boundary rejection,
not a fabricated specification condition. The existing shared YAML decoder formatter also feeds document preparation:
duplicate keys, forbidden constructs, invalid Unicode and non-ASCII source
positions retain their independent diagnostic truth. Non-ASCII diagnostics
include the captured source identity. Complete encoded capture replies are
bounded before returning to a host; a partial JSON prefix is never published.
Inherited preparation retains the admitted schema with failures. Layer errors
resolve against their original input; later normalization/dependency failures
retain the exact failed pass arena only when semantic context needs it.
Composition retains its own context document. The shared adapter resolves
findings and source/entry/parent provenance into the same portable issue records
as standalone preparation. Opaque host errors and interruptions retain their
original payloads; resource/transport outcomes remain separate. Independent
traversal truth and authored failed-pass vectors cover these paths in both hosts.
Capture IO failures, whole-run report assembly, and the public issues-table
frontend still have separate integration gates. This does not qualify a public frontend or promote
any benchmark inventory row.

## Classified resource failures

A source port can explicitly classify a failed capture as a missing resource or
a non-regular file. The engine retains that typed cause alongside the original
error payload and actual capture counters. Shared core diagnostics supply the
condition, validation phase, REQ-0785 and authored dataset/path context; the
report adapter preserves prior requests and refuses publication for the failed
build. It never obtains a classification from an exception message.

Ports default to no classification, so existing Python/R callback exceptions and
interruptions remain opaque. Resource limits, malformed accounting and internal
failures are not language findings. This mapping changes neither filesystem
authority nor resolution order. Production filesystem adapters, preflight file
checks before study reads, additional IO causes, and public host integration
remain required; the callback prototypes do not gain file authority here.
