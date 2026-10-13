# Native application ports

The #1755 application foundation moves the original-document
capture/decode/bind/execute lifecycle into `yamaa-engine::specification_run`
and output gates into `yamaa-engine::specification_output`.
Python and R use that service through the existing bounded facade. Its resource
and codec interfaces are native traits; adapters translate values and errors.

| Port | Owner | Current implementations | Failures |
| --- | --- | --- | --- |
| `specification_run::SourcePort` | engine | Python/R metadata and capture bridges; descriptor-backed Python resource adapter; Unix Rust file lifecycle in both hosts; native fakes | Known inspection findings in declaration order; original opaque error; invalid snapshot counter |
| `specification_run::SourceDecoder` | engine | Adapter CSV/Parquet/Arrow decoder; engine test fake | Original decoder error; byte/cell capacity limits |
| `specification_output::ArtifactEncoder` | engine | Adapter CSV/Parquet encoders; native test fake | Original codec error; checked output byte limit |
| `specification_output::OutputReport` | engine | Portable JSON report formatter; native test fake | Observation or report-budget failure before publication |
| `specification_output::ArtifactPort` | engine | Python/R publication bridges; Unix explicit-target Rust/Python/R publisher; native test fake | Original opaque publication error, without retry |
| `TableAccess` | core | Immutable Arrow snapshots, core normalized CSV tables and engine output tables; test fakes | Bounds or original opaque cell-access error |
| `inheritance::SourcePort` | engine | Typed inheritance capture adapter; native captured-file port; remaining R JSON compatibility probe | Source failure or traversal resource limit |
| `project_activation::ActivationPort` | engine | Python/R locked-package adapters; native fakes | Original lock, binding, conformance or interrupt failure |
| `build::PreparationPort` | engine | One shared native metadata connector, for absent or explicit environments and recursive producers; native spies | Original retained preparation owner; sealed whole-entry selection before execution |
| `build::Ports` | engine | One shared native application with Python/R activation and result ports; native fakes | Preparation failure before study authority; checked activation/build/output/report failures remain distinct; no Build publication authority |
| `build::ResourceReader` | engine | One native metadata reader shared by original document/parent/producer/dictionary and environment/function/codelist/lock preparation | Original file/quota/snapshot failure; declaring identity and held bytes retained |
| `project_preparation::CapturePort` | engine | Adapter native file metadata capture; shared scripted native fake | Original capture/interrupt payload, bounded source/byte/identity refusal or malformed native reply kind |
| `project_preparation::DecodePort` | engine | Adapter captured YAML/schema codec; shared scripted native fake | Original structural failure with retained source/context; closed environment/function/codelist target |
| `function_invocation::FunctionPort` | engine | Python/R scalar callbacks and activated registries; `project_activation::Bindings`; native fakes | Original host exception, rejected representation or typed invocation failure |

The one function invocation port borrows immutable legacy or versionless
signatures by stable slot and calls that slot with declaration-order host
arguments. Signature inspection executes no project code. Dataset admission
checks every selected slot before source cardinality or cell access. Already-bound
scalar probes use slot zero through the same invocation service. One generic
`NoFunctions` implementation supplies no callable authority in every layer.

The opt-in engine `test-support` feature exposes shared scripted native ports.
Source, decoder, encoder, publisher and function fakes retain owned outcomes and
one shared ordered event log. They move original opaque errors without requiring
Clone, Send or Sync and never retry. Default engine unit tests exercise the same
capture, decode, execution and held-publication lifecycle; the feature also makes
the fakes available to integration tests without adding production dependencies.

`build::Request` owns the shared request shape. `build::prepare_environment`
composes the shared reader and structural codec with the environment application,
retaining capture and preparation failures separately. Metadata root capture resolves,
then captures and admits the original preparation copy through one reader;
dependency captures keep their original declaring source. `build::select_entry`
charges the complete retained original arena, admits normalized model capacity,
inspects the whole authored input collection and verifies held snapshots before
returning its sealed route fact. Schema defaults cannot invent producer inputs.
Neither operation grants activation, study-data or publication authority.

`build::prepare` owns route selection from that sealed original-entry fact.
An ordinary entry without an environment keeps its plain checked capability;
explicit environments use the checked project, and producer graphs use the
checked graph with either an explicit or absent environment. Absence has no root
capture, dependency, lock or discovery location; producer reports retain a null
source identity. Metadata is captured once and moved into the selected capability.

`build::domain` and `build::check`, also exposed by `domain_entry`, are the one
Build and Check entry points for both installed hosts. Their shared native
application composes the metadata connector and static checks with host activation
and result-conversion ports. The bindings convert arguments and owned results;
they do not select or reconstruct a route. Existing checked engine services own
lock/binding/function-test admission before study capture, source decoding,
execution and output gates. Report and public projection complete before the held
result exposes explicit Save. No publisher is accepted during Build or Check;
Save uses only the completed result's held bytes and preserves original failures
and publication prefixes without replay. Native spies cover all four combinations
of producer/ordinary and explicit/absent environment, and original non-Clone
failures stop subsequent effects.

Trusted application observers receive stage transitions separately from semantic
reports. `application_observer::Scope` carries a shared observer and, for producer
work, the actual borrowed graph node index and identity. `domain_with_observer`
marks Preparation before native metadata admission, Build before entering the
selected runtime port, and Finished after a returned success or failure. The
checked source service marks its actual inspection/capture/decode/bind boundary
and forwards the existing dataset Admission, Derivation, OutputKeys, Verification
and Finished events. Project and producer services mark activation only when
called definitions enter that gate; producer Output begins its actual report and
artifact preparation. Ordinary output/report clocks are retained by the adapter.

FunctionCall events occur immediately before delegated FunctionPort calls, or
ActivationPort invocations during authored cases. Signature lookup and rejected
or short-circuited argument admission produce no call event. Returned failures
and unwinds keep their actual entered prefix and original native payload; a panic
can omit Finished. Existing APIs use the no-op observer. Engine observers own no
clock, allocate no measurement storage and alter no report, quota or Save
authority. Adapters retain their bounded clocks and counts privately; Save
measurement remains a separate actual held-owner operation.

Typed Exit events preserve actual completion and failure independently of phase
transitions. Dataset admission, derivation and output-key stages complete at their
actual successful checkpoints; the last entered stage exits from its returned
result before Dataset Finished. Input, activation, node output and enclosing
request preparation/build use their actual returned results. FunctionReturn
records the delegated port result without inspecting its opaque payload. A
missing argument has no call/return pair, and an invocation or stage that unwinds
has an unmatched entry rather than an invented failed return. These measurement
outcomes do not replace diagnostic phases, findings or original error owners.
An optional Refusal event copies only an existing classified execution limit's
resource, limit and optional required amount before its failed exit. It neither
invents a used counter nor classifies generic Capacity or opaque port failures.

## Current lifecycle

The bounded `yamaa-engine::domain` use case now owns executable preflight.
`check` consumes an already admitted normalized model and returns an immutable
`CheckedSpecification`; it has no study-data, function or publication port.
`build` performs that same check before invoking source authority. Both installed
host adapters retain the checked capability and call its `build_into` operation
inside their existing panic boundary, so partial source observations survive.
Native fake-port tests pin unsupported-before-capture behavior, cached repeated
builds, opaque error identity and incomplete attempts after a host panic.

`project_preparation::prepare` owns environment declaration lowering, dependency
capture order, cumulative source/byte/identity/node accounting, function/codelist
lowering and final environment admission. Root structural and scalar gates precede
every dependency request. Declared capture-count admission precedes the first
port, including attempts that fail. Lock capture is first, followed by external
functions and codelists in written order; inline definitions retain the same
authored coordinates. The engine supplies remaining byte and identity allowances
before capture and rejects oversized replies before decoding. It charges decoded
nodes before lowering or retaining a definition. Ordinary independent source
failures collect; original interrupts abort immediately without later authority.

Success and rejection move the original captured bytes, normalized documents,
origins and opaque errors. A failed authored lock capture does not invent an
absent-lock finding. The service has no activation, study, invocation, publication
or retry port. The adapter implements structural decoding and preserves its
existing result/error shapes; it no longer assembles environment drafts or owns
this preparation loop. Shared scripted metadata ports pin effect order and exact
remaining allowances and consume non-Clone outcomes once. Engine tests use
independent normalized documents; adapter tests keep original YAML truth.

Captured schema closure, standalone preparation and raw inheritance capture/rebase
now use the dedicated engine services described below. Authored-origin lookup and
class preparation remain separate migration work. Existing engine inheritance
trait implementations remain codec/resource adapters. The
Python inheritance JSON callbacks are retired after their qualification consumers
move to typed native preparation and retained-arena oracles. The R JSON callback
probe and shared transport remain for owner-coordinated retirement; Python-only
removal does not close the No JSON host callback criterion. Other schema query
component probes and custom/reference resolver dispositions remain unchanged.

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

Ordinary named selections without derived materialization charge the choices
cache for one lookup and, on a miss, one insertion search plus three new typed
fields. No prior entries or donor values/text are copied into a choice. The
tuple comparison admits its identity tag, identity value and intermediate index.
The pinned Rust 1.90 B-tree has eleven keys per node, at least five keys/six edges
in a non-root node, and a linear node search: see its
[node layout](https://github.com/rust-lang/rust/blob/1.90.0/library/alloc/src/collections/btree/node.rs)
and [search](https://github.com/rust-lang/rust/blob/1.90.0/library/alloc/src/collections/btree/search.rs).
Twelve comparisons at each of `1 + ceil(log6(max(entries, 1)))` rounded levels
bound each search, including the root and unsuccessful searches. Thus each new
ordinary choice consumes at least 75 logical work units. Existing identity,
key-cell, string and selected-value admission remains cumulative. Expression
keys and all derived contexts retain their prior linear cache admission;
current-dependent materialized schema/text retention needs a separate guard.

The ordinary cache's held storage is bounded separately from work. On the
qualified 64-bit Rust 1.90 hosts, the actual typed choice entry is 40 bytes and
the string/bucket entry is 48 bytes. Including unused node slots and all twelve
internal edges, conservative node sizes are respectively 552 and 640 bytes;
numeric-key/vector or numeric-key/index maps need at most 464 bytes per node.
With `N(n) = 1 + floor(n/5)`, a tree with `n` entries has at most `N(n)` nodes,
including an allocated empty root. For ordinary-only attempts, let `C` be
choices, `E` eligibility entries, `I` indexes, `G` retained distinct string
groups, and `R` the sum of input-row capacities of held eligibility vectors.
Let `W`, `K`, and `T` be existing work-cell, key-cell, and scalar-text limits.
Then `C <= floor(W/75)`, `I <= floor(K/128)`, `G <= K`, and
`R <= min(W, E * source_rows)`; `E` also cannot exceed admitted intermediate
declarations. Index bucket capacities hold at most `2*K` coordinates, and
copied key strings at most `T` bytes. A conservative requested-storage bound is
`104 + 552*N(C) + 464*N(E) + 8*R + 464*N(I) + 640*(I + floor(G/5)) + 16*K + T`
bytes. It includes map metadata, unused node capacity, string storage and row
coordinate capacity. Allocator metadata/rounding and other retained execution
objects belong to measured process RSS, not this logical cache bound. SELF
invalidation still charges the actual linear visit to held entries. No quota
is increased by this accounting change.

Core owns pure CSV profile admission, declared type admission/conversion, and
their closed failure causes and diagnostic context. Its immutable typed rows own
normalized values independently of source bytes and declarations. The CSV adapter
retains physical Arrow representation and checks the existing table capacities
before conversion. Profile behavior and effect ordering are preserved. The held
Parquet decoder translates storage metadata into the core-owned closed type
profile and temporal admission. It uses the compiler's empty-string policy, with
bounded physical decoding and ordered semantic findings described in [PARQUET_SOURCE.md](PARQUET_SOURCE.md).
The adapter serializes the engine result through the existing response
formatter; it no longer chooses capture, binding or execution order. Tests use
native fake ports to pin order, cached counts, original non-Clone errors,
capacity failures before cell reads, and observations surviving a panic in a
later source. The portable report preserves every requested capture and the
engine's actual handler counts, including inherited named-selection handlers.

## Output lifecycle

The bounded original-YAML producer route builds the complete graph in memory.
Its consumers ingest the producer's retained serialized artifact, including CSV
reporting precision, and public root output keeps its original table precision.
Build writes no artifacts. Explicit Save preflights the complete target set, then
publishes held bytes in producer-before-consumer order without recapturing inputs,
reencoding artifacts, rerunning activation or invoking functions.

`producer_publication::PreparedPublication` has no public constructor. Only the
actual completed engine build creates it; the native adapter seals that capability
beside immutable complete build evidence. Caller-authored public attempt records
remain evidence and cannot gain Save authority. Report and root-output projection
must finish before the host result can expose Save. Cumulative node, path, held-byte
and work limits, duplicate normalized targets and publication-report admission are
checked before the first write.
Native target admission reserves both retained canonical path copies before
resolving locations. Windows duplicate-target comparison uses the transport's
ordinal case-insensitive equality, including targets that do not yet exist.
Whole-set Save preflight also compares held physical parent identities, so distinct
selected root spellings cannot bypass target collision admission. On macOS the
held parent's native `fpathconf(_PC_CASE_SENSITIVE)` policy governs absent ASCII
case aliases. Two targets sharing one macOS physical parent conservatively refuse
if either basename is non-ASCII: their Unicode normalization distinction is not
qualified. This limitation follows Apple's [APFS filename rules](https://developer.apple.com/library/archive/documentation/FileManagement/Conceptual/APFS_Guide/FAQ/FAQ.html),
which include normalization-insensitive lookup on case-sensitive volumes.
Installed supplemental witnesses retain complete native producer reports and
separate actual saved-file bytes with their checked source/installed-artifact
provenance. They do not satisfy the separate formal M1 producer-cohort criterion.

Each Save retains its actual entered prefix, including the failing call, and the
original publication error or unwind. A failure stops later dependent writes;
earlier successful writes and the completed build remain. Repeating Save clears
only the previous publication result and repeats publication from the same bytes.
This qualifies the original rounded CSV alias/diamond fixture; general workflows,
other codec combinations, sidecar publication and concurrent Save remain separate
migration work.

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
the public facade now connects owned output, issue and
[log views](VERIFICATION_LOGS.md), with complete-target preflight and actual
publication-prefix retention. Broader API qualification remains open.

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
path selects the encoder from the declared output extension; Parquet input uses
the held decoder described in [PARQUET_SOURCE.md](PARQUET_SOURCE.md).

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

The existing inheritance JSON callback bridge still has qualification and
reference-route consumers. Their replacement and retirement remain open under
#1751/#1755. This function-port consolidation and reusable test support do not
complete that retirement or change a public default or golden fixture.

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

Ports default to no classification. Both installed host bridges additionally
admit an explicit closed failure reply: `("missing", original_error)` or
`("not_regular_file", original_error)` in Python, and the corresponding list of
kind and original condition in R. The bridges retain the payload and pass the
typed cause to the core; they do not infer causes from messages or class names.
Thrown errors, interrupts and returned interrupt conditions retain their original
host identity. Resource limits, malformed accounting and internal failures remain
boundary errors. This mapping changes neither filesystem authority nor resolution
order. Preflight file checks before study reads, additional IO causes and public
frontend integration remain required.

Python's existing descriptor-anchored `ProjectResources.capture` accepts an
optional trusted byte ceiling. It checks file size before reading and stops at
one byte beyond that ceiling if the file grows. Refusal is an opaque resource
policy error, creates no snapshot and adds no cached alias to verification.
Bounded reads use small chunks rather than allocating the entire ceiling.
Verification compares at most the retained length plus one byte, directly, so
an extended file cannot force an unbounded read. Approved roots, link checks,
relative-path fallback and default capture behavior retain their existing rules.
The internal Python `SourceCapture` adapter connects that resource reader to the
shared build port, verifies held bytes before every decode and explicitly returns
only the two represented filesystem causes. Package, IO and specification exports
load lazily so importing this adapter does not load the reference parser, planner,
evaluator or table codecs. Installed qualification exercises actual missing and
non-regular files at the first and later source request, cached counters, complete
failed reports and retained save gates. The metadata-only `inspect` operation checks every declared source before the
first study capture. Known missing/nonregular causes collect in declaration order;
opaque errors and interrupts stop before later authority. Inspection cannot change
the capture counter. A failed preflight retains no source-read or table observations
and cannot be saved. Hosts may omit inspection for existing explicit-byte callbacks,
preserving their qualified request timing. Seven independently authored complete
reports match actual reference-file failures, including multiple failures; installed
hosts pin original error/interrupt identity, malformed replies and repeated builds.
Python exercises descriptor-backed metadata inspection without reference semantic
imports. The Unix file lifecycle supplies native entry/layer preparation and the same metadata/capture port to both hosts; environment/codelist filesystem preparation, environment/function preflight and the public facade remain open. Existing reference APIs keep their names and behavior until cutover.


## Native Unix file resources

`yamaa-adapters::file_resources::Resources` implements the engine source trait and
backs the private registered R file-resource bridge. Approved root descriptors stay
held for the adapter lifetime; individual components open without following links,
and every parent/file identity is checked again before accepting the file. Only
an absent entry allows a relative fallback. Observed permission and wrong-kind
failures are terminal; permissions and generic IO remain opaque unreadable errors.
The architecture guard admits only the pinned Unix filesystem dependency in this
adapter, retaining pure core/engine boundaries. First captures check size before allocation, read bounded
chunks and compare a fresh read directly. Reuse verifies all retained aliases;
limits and changes never create a snapshot. Declaring-file views share the same
selected roots and byte store; every accepted base/path spelling is retained even
when two spellings resolve to one key. Metadata resolution does not capture bytes.
Physical cache hits require a current device/inode witness and valid retained
aliases. Stale physical entries cannot poison a never-captured path, while old
path keys retain their snapshot and fail closed. The adapter interprets no study data.

The [qualification scope](crates/yamaa-adapters/tests/fixtures/native_file_resources.md)
pins real CSV/Parquet input, complete independent reports, cached counters, direct
byte equality and registered handle lifetime in installed R without Python. The
explicit-root probes retain the historical approved-root/fallback qualification
behavior. Replacement public routes select `Resources::for_entry` and follow
[#1905](https://github.com/elong0527/yamaa/issues/1905): each original declaring
file supplies its own relative location, absolute/parent-relative local paths
are allowed, and absent files have no fallback. The same native kind, link,
snapshot and held-byte publication checks apply in both modes. Windows R
installed release qualification remains separate under #1903.


## Explicit-target native publication

`yamaa-adapters::file_publication::Publisher` implements the engine artifact port
for one absolute file explicitly selected by its caller. It retains that parent
directory, checks the declared path and byte capacity before effects, coordinates
native writers through a nonblocking directory lock, and stages complete bytes in
a private owned directory beneath that parent. The private directory descriptor
binds the checked candidate to the final rename even if its parent entry is
replaced. Staging names equal to the target basename are skipped before creation.
A prior artifact survives errors before replacement. Explicit cleanup checks
candidate and staging identities before removal; unconfirmed or replaced entries
are preserved. A cleanup failure retains the original operation error, cleanup
cause and captured staging location instead of claiming that no residue remains.
Existing hard-link aliases keep their prior
bytes because publication replaces the directory entry.

The private R bridge can save an owned build result directly through this native
port without an R publication callback. The [qualification scope](crates/yamaa-adapters/tests/fixtures/file_publication.md)
covers complete independent CSV reports, retained Parquet bytes, repeated saves,
failed-build rejection, prior-artifact preservation and registered handle lifetime.
Public save integration and Windows native publication remain open.


## Shared native file preparation

`file_preparation::FileSpecification` owns a prepared model and retained approved-root
resources. Both private bindings marshal paths and metadata only. The shared service
captures entry and parent YAML, selects the shipped schema and preparation lifecycle,
retains parent provenance, lexically rebases inherited paths and compiles before
study authority. Checks use the retained compiled model without reading study data.
Build repeats metadata inspection and cached snapshot verification, and returns an
owned result for explicit native save. Opaque native resource failures remain boundary
errors; the two closed portable resource causes use complete failed observations.

The [qualification scope](crates/yamaa-adapters/tests/fixtures/file_preparation.md)
covers seventeen complete original reports in both installed Unix hosts, independent
early findings, cached counters, exact saved CSV and retained failed-save gates. R
runs without Python and uses registered handles. The public facade, Windows native
file transport and complete environment/function/workflow/submission semantics remain
open. No reference-assisted qualification level or path authorization is promoted.

`inheritance_capture::prepare` now owns the raw inheritance capture application
around the existing traversal/composition lifecycle. It borrows the exact already
decoded entry, reuses `project_preparation::Source`, admits cumulative captured
bytes and identity text, and moves successful parent snapshots into a retained
catalog. Parent decoding happens once per requested unique canonical source;
raw snapshots remain in first-decode order while normalized layers remain in
postorder. The decoder port retains physical YAML limits and source positions.
Its closed limit-error constructor preserves the existing adapter representation
without classifying host errors. Neither raw byte admission nor identity admission
is substituted for traversal, normalization, document-storage or rebasing policy.

The fail-fast capture port preserves separate canonicalize, capture and rebase
operations, including their original unavailable/raised distinction. The engine
supplies remaining capture bytes and forwards the lifecycle's remaining rebasing
allowance. Original host and decoder payloads move without Clone, Debug, Send or
Sync requirements. A later failure retains the completed raw-parent catalog;
opaque failures stop immediately, with no independent-error collection or retry.
The adapter retains its public InheritancePort/CapturedParent/SourceFailure and
InheritanceError shapes through reexports and a physical decoder implementation.
The existing typed Python/R native preparation callbacks retain their signatures.
Python module/snapshot inheritance JSON source callbacks are retired; the R-only
component JSON bridge remains separately scheduled for removal.

Authored-origin lookup helpers and the native Parents spelling cache remain
separate migration work. The post-success
native alias checks and entry-base selection stay in file_preparation. Existing
YAML scanner rules, report/log projection, producer facades and filesystem policy
are not part of this move. Inheritance callback component probes remain pending
retirement under #1751; this application service does not qualify their API as a
permanent public surface or close the remaining #1754 acceptance gates.

`schema_capture::admit` owns admission of the complete captured schema closure
before any physical decoding. The default root retains its existing absence of a
separate root-name charge; a named root checks that text before module admission.
Module count then per-source captured bytes and identity text are admitted in
supplied order, with bytes checked first. Each module is decoded once through a
physical codec port. Only after every physical decode succeeds does unchanged
core bundle admission resolve entry/module/include/descriptor semantics. The
original source snapshots, decoded arenas and occurrence locations move into the
admitted closure without replacement; defaults retain their original module and
descriptor provenance and keep their later normalization gate.

This service does not resolve files or scan YAML. Codec limits and the guarantee
that occurrence locations address the exact decoded arena remain codec authority.
Original opaque codec payloads move without Clone, Debug, Send or Sync requirements.
The adapter retains its public CapturedSchema/Source/Error shapes and only maps
closed resource categories and physical decoding. Default resource policies and
configured larger module capacities remain unchanged; there is no new source
ceiling or retry. Class normalization, lifecycle selection and authored-origin
helpers remain separate migration work, as do native alias/cache/entry-base
policy and component probe retirement. This slice does not close #1754.


`standalone_preparation::prepare` owns the no-parent structural lifecycle over one
owned raw document and the admitted schema. Presence of `parents`, including null,
rejects before any normalization charge. Normalization, strict named-window
expansion, origin remapping and final model admission preserve their checkpoint
order and share one caller-owned NormalizationBudget. Existing storage, work,
depth and diagnostic policies stay distinct; no limits or counters are reset.
Default and window-definition/use-site origins continue to address the retained
raw/schema arenas. The remapped origin list retains its existing bound from the
already-admitted expanded arena rather than introducing a new metadata quota.

Success moves the original raw allocation into the result. A semantic
normalization or expansion failure moves the exact failed-pass input arena with
its complete ordered findings. Policy/internal errors retain their typed identity
without a document copy; final model findings retain the existing document-free
representation. The adapter only moves those results into its CapturedFindings
and PreparedDocument shapes and rejoins physical occurrence locations with the
unchanged raw arena. Entry physical decoding/source limits, lifecycle dispatch,
class/structural preparation and inheritance remain unchanged. This service has
no host port and introduces no opaque-error classification or callback effect.

Standalone lifecycle ownership does not close the broader #1754 acceptance gates.
Class preparation, dispatch, authored-origin helpers, native alias/cache/entry-base
policy and component probe retirement remain separate work. YAML scanning,
producer/source-origin facades, report/log projection, Define and Windows behavior
are outside this slice.

`structural_preparation::prepare` owns the live environment root and external
function/codelist class normalization route used by project_preparation's codec.
A named request admits class text, looks up only a closed class and rejects any
class declaring schema_version before physical decoding. Structural root retains
its separate decode-first version/default gate and has no named-selection check.
The physical codec calls the unchanged entry decoder once, preserving captured
byte-before-identity limits, YAML parsing limits and original occurrence locations.
After decoding, one fresh per-document NormalizationBudget spans named type-query
binding and normalization. Existing project preparation's cumulative source, node
and semantic budgets remain separate; no limit or counter is reset within a pass.

The immutable engine result moves the original source bytes/identity, raw arena
and location payload with its normalized tree. Complete semantic findings move
the exact raw failed-pass arena and source into the existing adapter representation.
Query validation and normalization policy/default failures keep their distinct
categories; generic original codec errors move without Clone, Debug, Send or Sync
requirements. Input origins address that retained raw arena, and Default origins
require the caller to retain the exact admitted schema module/descriptor authority.
This lifetime/retention contract does not authenticate an equal replacement arena.

The adapter preserves CapturedDocument/CapturedFindings/Error shapes, the existing
prepare_class convenience delegate and all standalone/inherited dispatch. It no
longer calls core query/normalization in these live entry points. The former private
two-caller cloning failure helper is removed when its callers move to the engine.
No source capture, activation, study or publication capability is added here.
The decoded schema/1 transports and host _Schema.analyze/interpret_schema
component probes remain for planned #1751 retirement. Python inheritance JSON
callbacks are removed; R registration/export and shared transport removal remain
owner-coordinated work. Dispatch, authored/producer-origin helpers, native alias/cache/entry-base
policy and other owners' scanner/report/log/Define/Save/Windows work remain separate
#1754 debt; this move does not claim whole-issue closure.

The live document preparation gate delegates to `document_preparation::prepare`.
Its entry codec addresses the exact retained raw arena, decodes the entry once,
and keeps its physical source/location admission separate from parent capture.
The engine selects inheritance by raw `parents` presence, including null, scalar
and empty values. It composes the existing standalone and inheritance services
without moving version/default gates ahead of that selection. Standalone passes
share one existing normalization budget; inherited capture, traversal and
normalization keep their existing separate cumulative policies. Source, raw
arenas, opaque locations and original port errors move without new clone, debug
or thread-safety bounds. Equal strings or a lifetime do not establish arena
provenance; the codec must expose the exact held document.

`file_preparation::prepare` owns entry capture ordering and the private parent
alias ledger. Native `Resources` remains the physical authority for resolution,
snapshots, path spelling, byte equality, descriptor witnesses and resource policy.
Entry resolve/capture/admitted preparation copy precede schema selection. Every
parent canonicalization charges identity/display metadata; a distinct alias pair
is admitted before its ledger key/pair copies. Duplicate pairs are compared by
borrowed strings. No new count ceiling or budget reset is introduced, and failed
native charges keep their original spent counters. The document connector only
selects held/package schema authority and delegates to the document application.

After semantic success the application borrows the immutable first-capture
parent catalog and verifies every distinct authored alias in encounter order,
including each first capture, without another preparation byte copy. Only after
all aliases succeed does it select the original entry base. A missing ledger key
is an explicit native representation refusal; early/late opaque errors move and
stop subsequent effects. This service does not add a root-byte reread: the
existing native entry-base verification remains the policy. Physical codecs,
authored producer origins, reporting/publication and compatibility component
entry points remain their separate boundaries.

The shared-work prototype gives entered producer datasets one non-cloneable
`execution_work::WorkBudget` with the existing whole-graph WorkCells ceiling.
Each fresh runtime starts its work counter at the previous actual admitted
amount. Every returned success or failure commits that counter; a refused charge
never counts a visit which did not occur. Before entering derivation, the owner
reserves its remaining ceiling, so an unwind cannot grant that work again. The
original error or unwind payload is neither inspected nor replaced. Metadata
and source-cardinality admission precede this reservation. Predicate work shares
the same cumulative counter; resolutions, regex work, text and identities retain
their existing per-node policies. The ordinary standalone APIs keep fresh scopes.

Source rows are reserved before activation using admitted input slots:
`floor(total_rows * node_input_slots / all_input_slots)`. The implementation uses
checked quotient/remainder arithmetic, and the sum of reservations cannot exceed
the existing total. Source aliases still consume their declared row/cell ingress;
no study value or capture effect selects the weights. All other node quotas and
whole-graph byte/cell/publication ceilings remain unchanged. This prototype
changes how the bounded work and row allowances are allocated; it raises no
ceiling and grants no publication authority. Existing expression/derived cache
charges remain untouched, and their separately unqualified retained-schema/text
scope is not accepted by this prototype.

The observed producer path forwards its actual node Scope and shared WorkBudget
through the same entry. Typed dataset phase/exit/refusal events and once-delegated
callback entry/return events remain active under cumulative work admission. A
21-cell graph control preserves the first successful node and both real callback
returns, then reports the root OutputKeys WorkCells refusal with global required
22 immediately before its failed exit; no root verification or encoding occurs.
