# Native application ports

The #1755 application foundation moves the original-document
capture/decode/bind/execute lifecycle into `yamaa-engine::specification_run`
and output gates into `yamaa-engine::specification_output`.
Python and R use that service through the existing bounded facade. Its resource
and codec interfaces are native traits; adapters translate values and errors.

| Port | Owner | Current implementations | Failures |
| --- | --- | --- | --- |
| `specification_run::SourcePort` | engine | Python and R capture bridges; engine and adapter test fakes | Original opaque capture error; regressing snapshot counter |
| `specification_run::SourceDecoder` | engine | Adapter CSV/Arrow decoder; engine test fake | Original decoder error; byte/cell capacity limits |
| `specification_output::ArtifactEncoder` | engine | Adapter CSV encoder; native test fake | Original codec error; checked output byte limit |
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
settings. The adapter serializes the engine result through the existing response
formatter; it no longer chooses capture, binding or execution order. Tests use
native fake ports to pin order, cached counts, original non-Clone errors,
capacity failures before cell reads, and observations surviving a panic in a
later source. The portable report preserves every requested capture and the
engine's actual handler counts, including inherited named-selection handlers.

## Output lifecycle

Successful response preparation retains the engine's accepted typed execution.
Publication does not infer acceptance from serialized JSON or decode its own IPC
output. An unsuccessful attempt reaches only the failure-report formatter.
For an accepted execution the engine orders initial observations, core output
findings, projection resolution, bounded encoding, complete bounded report
preparation, and one publication request. An error at any step stops later
requests. The engine independently rejects a codec result beyond the supplied
byte limit, even if the codec violated its allocation contract.

Report formatting and CSV representation remain adapter responsibilities. A
report error, including its size budget, must be returned before publication.
Opaque codec/report/publication failures are retained without cloning or retry.
The current report still uses the existing portable transport for diagnostic
observations; replacing that formatting transport is a later cleanup. Native
fake ports pin declaration order, rejection, codec overruns, error identity,
report-before-publication order and repeated explicit requests.

## Remaining migration

This is an exercised source-read foundation, not completion of #1755. The
unified resource reader still needs specification/layer/producer/environment and
codelist resources, with resolved paths and declaring-file provenance. Current
source bridges retain their existing path behavior; the #1751 target resolves
each path relative to its declaring file without an approved-root boundary.

Parquet encoding/decoding, general publication/save policy, environment
activation, shared reusable test fakes, and consolidation of the two function
interfaces remain open. The existing inheritance JSON callback bridge also
remains to be replaced. No new unimplemented port is presented as a working
capability, and no public default, golden fixture or qualification level changes.
