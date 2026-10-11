# Portable diagnostic foundation

Known verification declaration findings now project through eight additional
canonical causes for invalid IDs, required payloads, repeated IDs, bounds, cardinality, column
types and unknown declared columns. The shared checker queries these owned findings
without study authority. A build keeps its earlier arithmetic/conversion failure
and its existing declaration checkpoint order. The thin issue encoder sends JSON
context text identically to both native hosts and refuses output above its byte
ceiling without returning a partial payload.

Column pattern failures now reach REQ-0379 through the pure completed-string
service. Invalid compiled pattern syntax owns REQ-0827 with its authored pattern
and pattern path, without parser offsets or reason fields. Both findings retain
reference checkpoint order. The shared matcher charges cumulative subject bytes,
work and state cells across checks and predicates; a policy refusal preserves the
completed prefix and does not become a false match failure.

Allowed-value, finite-range and Unicode-length checks now own REQ-0376, REQ-0377
and REQ-0378 through the same pure completed-value service. Their core projection
retains column and failure count, plus the authored maximum for length. The engine
admits potential comparison work and text scanning before evaluation; a capacity
refusal keeps earlier completed records and stays distinct from semantic failure.

Column presence checks now reach `VerificationNotMissingFailed` through the pure
completed-value service. Core owns REQ-0375 and its column/failure-count context;
the report adapter adds observed keys and the authored verification ID. Completed
column records remain attached to an attempted run when a later conversion or
other error stops derivation. A later declaration error preserves earlier failed
records while retaining its own primary diagnostic.

The #1753 slices move numeric evaluation, arithmetic/reduction, completed-result
conversion, classified resource, original-document preflight, output-declaration
numeric/aggregate grammar, binding/dependency, CSV profile, window and
named-selection/predicate grammar and declared CSV source typing and closed Parquet profile diagnostics into
`yamaa-core::diagnostic`. The
original-specification runner and existing dataset/numeric transports consume
those core diagnostics. Adapters encode runtime values and source offsets but
do not reconstruct these families' semantic context or choose their requirements.

`Diagnostic` owns scalar and ordered sequence context, ordered specification paths, optional UTF-8
source span and optional operand route. Diagnostic integers outside runtime i64
remain canonical decimal text with a distinct type. Absent source geometry stays
absent. Original opaque resolver errors have no normative projection: the caller
retains the error and its identity rather than manufacturing `unknown_field`.

Key-grain distinct-value conflicts, implicit lookup zero/duplicate matches,
baseline ties and present nonnumeric reduction arguments now project through
five core dataset causes. The engine borrows its retained typed failure and
admits the complete diagnostic before any path or context copy, preserving
original error precedence and table effects. Counts that exceed i64 remain
mathematical diagnostic integers. Matched keys, identities and partition
observations stay attached to the original attempted run; the dataset/1 adapter
serializes the common finding with its established context representation.

## Registry identity and compatibility

Each `ConditionCode` names one semantic cause and maps to one phase, public
condition and optional requirement in the registry. A public condition string is not a
unique registry key in the existing language:

| Semantic cause | Public condition | Requirement |
| --- | --- | --- |
| Invalid conversion input/text | `conversion_failed` | REQ-0013 |
| Nonintegral or out-of-range integer conversion | `conversion_failed` | REQ-0021 |
| Invalid temporal conversion text | `conversion_failed` | REQ-0601 |
| Nonnumeric compute input | `incompatible_input_type` | REQ-0444 |
| Floating-point decimal-rounding digits | `incompatible_input_type` | REQ-0418 |

Collapsing these mappings would change independent expected diagnostics. Internal
cause identities preserve the distinctions without adding a new wire field or
changing public condition names. The existing `phase`, `condition` and
`requirement` methods delegate to the same registry as the diagnostic projection.
Registry identity has no serialization or content-hash role.

Original preflight findings for undeclared row columns and unavailable drivers
retain their existing null requirement. Assigning a requirement merely to make
the registry non-null would change independent truth. The core projection also
preserves authored row indexes, column names, ordered multi-path conflicts,
missing driver values and empty/repeated row or group lists. The report adapter
represents these typed findings as the existing JSON shape. Resource failures
use that same projection without a tagged-scalar encode/decode round trip.

## Evidence and remaining work

Captured structural schema reports resolve context in core against the exact
caller-held pass arena and admitted schema. Core admits the complete ordered
report before adapters copy its borrowed scalar or permitted-list facts. Default
references require their descriptor's declaring-module arena; paths never infer
that provenance. Mathematical integers, distinct value/layer kind vocabularies
and duplicate permitted values retain their existing meaning. Invalid references
or duplicate context/provenance names refuse separately from byte exhaustion.
The schema prototype's path/context-reference wrapper remains compatible.

The core integration test reaches every registered cause through evaluation,
conversion, resource classification or actual original-document compilation and
compares independently stated public vocabulary and preflight context. It also checks
nested source location, literal/arithmetic integer overflow, a 5,000-digit failed
conversion and an opaque non-Clone host error whose identity and first-failure
order survive diagnostic inspection. Adding a registry entry without a reached
test fails that coverage assertion. The unchanged shared transport/lifecycle and
original-document fixtures continue to pin exact complete outcomes.

Five authored preflight documents carry sixteen complete ordered findings from
the independent reference implementation in `tests/fixtures/preflight.tsv`. Rust
and both installed hosts compare those complete envelopes before any source
authority is reached. The existing installed original-document suite separately
checks the Parquet redundant-type preflight finding; the core test exercises all
five declared type spellings. These are preparation and retained-run contracts,
not public API or whole-inventory qualification.

Five independent output-declaration documents carry eight complete ordered
findings in `tests/fixtures/output_declarations.tsv`. The reference implementation
produced these complete failure reports after source ingestion and derivation.
Core output findings now own their vocabulary and geometry; the report adapter
uses the same typed projection as preflight and resource failures. Rust and both
installed hosts compare the full reports, including source tables and capture
accounting, then require repeated save attempts to fail without publication or
recapture after the compiled handle is released. Output declaration checks still
follow derivation, key checks and verification.

Seven authored grammar documents in `tests/fixtures/grammar_diagnostics.tsv`
pin complete independent failed reports after ingestion and binding. Numeric
and aggregate parser findings own their context in core and delegate existing
condition/requirement accessors to the registry. Written function case, optional
argument count, prohibited construct and nested reducer names survive projection.
They retain distinct numeric and aggregate requirement mappings even when their
public condition strings match. Inconsistent caller-supplied function spans
produce no semantic projection; parser policy limits remain separate failures.

`tools/check_diagnostics.py` checks literal requirement IDs across every Rust
crate's source against current normative rule definitions. It also requires each
core registry condition and phase to be registered in `yaml/conditions.yaml`.
Adapter and binding source literals are bounded by exact existing source lines
and occurrence counts. Generated `shipped_schema_data.rs` is exempt from the
ownership ratchet; its requirements are still checked against normative rules.
The source scanner decodes ordinary Rust string escapes and raw strings, then
inspects JSON payload values, including nested encoded JSON and format templates.
Comments, character literals and quoted prose do not create diagnostic sites.

`tools/diagnostic_literal_seed.json` freezes the original sites. Do not add or
change seed sites. `tools/diagnostic_literal_allowlist.json` selects the remaining
sites and counts from that ceiling. When a cause moves into core, remove its
active entry or reduce its count; stale entries fail. Copying a seeded line,
changing its text or moving it to another file fails. Inserting unrelated lines
above a site does not consume a new allowance.

CI runs `python tools/check_diagnostics.py --baseline-ref HEAD^` from `rust/`,
with at least two Git revisions available. Baseline mode compares decoded records
directly: the frozen seed must match the prior revision, and an active allowance
may never grow or reactivate a retired site. The first introduction permits both
files to be absent only when the prior checker did not already require them.
An unavailable revision or an incomplete baseline fails. Without `--baseline-ref`,
local checks enforce the frozen ceiling and stale entries but cannot prove
historical non-growth. Native CI also runs the guard's mutation tests.
The existing canonical rules validator separately checks definition uniqueness
and historical IDs. The guard does not infer requirements from documentation
citations or count component tests as complete-run qualification.

This does not complete #1753. Schema, function, verification and remaining
ingestion diagnostics still need conversion; some remain constructed in adapters. Inherited source/entry/parent
provenance, additional structured context and unified serialization across all
protocols must migrate with those families. Existing per-protocol wire wrappers
remain temporarily for unmigrated errors. No registry completeness, public API
cutover, new numerical policy or benchmark inventory promotion is claimed.

Original output-window validation now projects through the common core diagnostic.
The canonical registry owns zero offset, required order and forbidden order;
`WindowFinding::definition` and its typed context use that same mapping. Three
independent complete failed reports retain authored paths, source reads, source
tables and save rejection. Named-selection reference/key-type and predicate
parser findings now retain the same shared diagnostic, preserving typed context,
original spellings and character positions without narrowing through runtime i64.
All original binding findings with normative projections use the common adapter
serializer. Resource policies and inconsistent metadata retain separate failures.

Classified packaging lock diagnostics now borrow their complete ordered host
facts and held root/captured/authored origins into core. Core owns reason tokens,
context fields, shape validation and whole-report admission before any authored
copy. uv/renv parsing, installed metadata selection and version comparison remain
host operations. The original opaque lock failure survives repeated projection
and quota refusal without a provider retry or binding/invocation effect; a
non-Clone, non-Debug, non-Send activation witness checks that identity directly.

The unchanged lookup reports and predicate syntax fixtures pin existing mappings
and richer parser context. Three independently captured malformed window
predicates now match complete original failed reports, including regex and date
rejection with the reference predicate/position fields. Core retains their rich
immutable parser facts and standalone syntax still emits those details.
`tests/fixtures/predicate_diagnostics.md` records capture provenance and the
remaining ESCAPE conflict: the unchanged reference reports REQ-0188 while the
native locked fixture and rule assign REQ-0191. Neither observation is rewritten.
REQ-0192's temporal grammar wording remains a separate reconciliation.

Native predicate evaluation now resolves all five semantic causes through the
core registry and projects their complete typed facts and explicit structural
route labels. Whole-envelope quota admission precedes copying authored paths,
identifiers and routes. Opaque resolver errors and resource failures remain
borrowed with their original identities, including under a zero projection
quota. Non-Clone, non-Debug source witnesses prove repeated inspection cannot
reread cells, run assignments or visit a later source record. The two written
BETWEEN value reads and upper resolver failure retain their original precedence.

Nine unchanged original corpus documents now replay through the private route.
Three negative window/row documents retain actual reference capture/ingestion
before binding failure, including repeated results and failed save gates. These
complete-report observations do not promote assisted inventory records or remove
the five assisted-route timing gaps.


Declared CSV source typing is a pure core service over the admitted CSV carrier.
Immutable declarations are checked before decode; unknown fields precede cell
conversion, which follows stored row/field order and never invokes a result
handler. Its owned normalized table outlives declarations and source bytes.
Core owns REQ-0532 and REQ-0536 context; structural carrier defects and declaration
policy failures have no semantic projection. The adapter retains resource checks
before conversion and Arrow allocation afterward. Four complete independent failed
reports in `tests/fixtures/source_typing.tsv` pin those boundaries and retained
save gates in Rust and both installed hosts.


The Parquet source profile admits portable physical/logical/representation
metadata in core. It owns legacy annotation compatibility, root alignment,
name/type precedence, flat-field admission and raw temporal bounds/precision.
The codec translates metadata and retains the existing diagnostic spelling;
physical decoding still finishes before profile admission. Metadata is consumed
lazily so root alignment and early field failures do not inspect later fields.
Five core registry causes project the existing malformed, empty/duplicate name,
unsupported stored type and raw temporal-value findings. Existing independent
Parquet files and complete failed reports remain unchanged, including source
collection and failed save gates. Physical decoding, compression, text validation
and Arrow construction retain their separate codec/resource boundaries.

CSV profile, declared CSV typing and closed Parquet findings also expose borrowed
facts for complete ordered report admission. Core charges every path, source
identity, field, stored-type spelling, position and complete failed value before
owned context materialization. Late quota refusal returns no diagnostic prefix.
The Parquet boundary lends its exact captured codec metadata directly; it no
longer clones an adapter error into a temporary core error just to project it.
The original scanner, schema/name precedence, row/field conversion order and
physical/temporal conversion remain unchanged. Opaque and resource failures
retain their original payloads and precede projection quota checks. A semantic
leaf without its held source declaration returns an explicit context refusal.
Callers must provide the declaration from the same failed attempt; matching a
name or path alone cannot establish that authority.

`source_diagnostics::findings_with_limit` and the ingestion branch of
`specification_diagnostics::findings_with_limit` use the admitted report. The
prototype failure envelope exposes context/limit refusal separately from a
language diagnostic. Existing owned `csv_source::Error::diagnostic`,
`typed_csv::Error::diagnostic`, core/adapter `parquet_source::Error::diagnostic`
and `specification_diagnostics::findings` preserve their former signatures and
unbounded compatibility behavior. They are scheduled for removal after their
remaining report callers migrate; the report/log ownership work is separate.
No existing capture, CSV/Parquet admission or host-output budget is replaced by
this diagnostic projection policy, and the non-ingestion preparation/binding
branches keep their existing policies.

Source-collection traversal has a separate structural admission ceiling of 128
active child-iterator frames. Its fixed borrowed frame array allocates no pending
sibling list and its footprint is independent of report breadth. Depth is checked
before adding a frame; excessive nesting returns `TraversalLimit` and the
prototype's `diagnostic_traversal_limit`, distinct from context quota refusal.
A reached opaque/resource barrier ends traversal immediately, preserving its
original precedence over later unreachable deep or oversized semantic facts.
For structurally admitted reports, even a late opaque/resource barrier precedes
the semantic context quota. A deeper collection reached before that barrier
refuses structurally rather than inspecting beyond the declared frame authority.


Driver source filters now bind through the core original-document compiler.
REQ-0132 rejects bare/wrong-dataset predicate identifiers; REQ-0148 rejects filters
over single output/intermediate values before inapplicable predicate parsing.
Both causes use the shared diagnostic registry. Missing stored fields and ordinary
predicate grammar use their existing canonical causes after complete ingestion.
Thirteen independent complete reports and three exact CSV outputs pin scope, null/empty
selection, raw distinct-reading behavior and scalar-filter precedence. The unchanged
`negative-source-missing-field` document has independently verified complete truth.
Filtered key assignments and secondary source expressions remain unsupported before
study effects; the existing predicate requirement/context gaps remain unqualified.


Original dataset assertion declarations now retain deferred core diagnostics
through the common engine and report projection. REQ-0405 names the first
lexicographically ordered unknown output reference; ordinary grammar retains
REQ-0188 and its original reason/character spelling. Valid when samples execute
before a later require declaration finding. Completed verification records survive
both declaration and sample failures. Seventeen independently authored complete
reports preserve these phase boundaries, failed-check key context and publication
rejection, including empty output and inactive guards. The special ESCAPE,
regex and temporal predicate parity gaps remain unqualified.


Original all_or_none verification failures now retain REQ-0382 through the
existing shared verification records and common report projection. Unknown
columns precede the two-distinct-name declaration check. Duplicate identities and
invalid declarations retain the completed check prefix; violated checks preserve
later records, shortened diagnostic keys and full offending-key log context.
Eleven complete independent reports and the unchanged negative-paired-dates
report pin typed source observations and failed publication gates in Rust and both
installed-host replay suites. Remaining verification families still need migration.


Static environment and retained scalar provenance now project in core through
`project_environment_diagnostics::projection`. The caller supplies the exact
held draft, normalized root, captured arenas, source identities and authored
origin slices from one failed attempt. A lifetime or equal source text does not
prove that relationship. Core validates the supplied ordered typed-to-authored
relationships and inline/external declaration kinds; the adapter checks retained
schema ownership. External function paths and codelist paths are selected from
borrowed semantic path pieces, without parsing rendered diagnostics. Failed
captures preserve authored index holes, and an invalid function name continues
to name the authored root declaration even when its definition was external.

Admission consumes each capture/source iterator once into a fixed array of at
most 128 borrowed descriptors, matching the existing adapter projection ceiling.
No width-proportional metadata vector, owned path, context text or scalar copy is
created before the complete report is admitted. Inline function count is separate:
200 inline functions retain all 600 independent findings. The existing 65,536
finding ceiling is unchanged, as is the explicit refusal for configured capture
collections beyond the existing projection ceiling; no prefix is returned.

The historical semantic raw-text policy and conservative issue-byte policy stay
separate. Root headers and root scalars keep separate semantic text budgets with
one aggregate issue budget; external scalar sources keep their per-source semantic
budgets with one aggregate issue budget. The admitted handles retain immutable
borrowed descriptors through materialization. Ordinary opaque capture failures
remain held alongside independent semantic findings, and semantic admission
resource errors retain their original precedence. This does not adopt ingestion's
opaque-barrier policy or change capture, static validation or activation effects.

The owned `root_diagnostics`, `scalar_diagnostics`, `diagnostics` and
`diagnostics_with_text_limit` APIs remain explicit compatibility routes with their
original signatures and raw-text/per-copy policy. They and the admitted report
share one closed semantic selector. Schema-root/source projections, cross-family
application aggregation and final serialization admission remain their existing
services. No registry vocabulary, Mapping representation or dependency is added.
Independent complete reports cover all static field routes, captured authority,
wide diagnostic integers, invalid temporal text, late quota refusal with zero
owned-fact allocations, and original non-Clone/non-Debug/non-Send host payloads.
Installed Python and R retain 200 valid inline definitions, complete six-record
static reports and five-record root/scalar reports. Their public route still
refuses the 600-record invalid-definition report before this projector: existing
`producer_check` aggregation reserves the entire normalized root for each finding
under its separate issue budget. The actual retained-capture adapter witness
admits all 600 records. That existing cross-family aggregation policy is unchanged.

## Inheritance traversal findings

Borrowed core findings own the five existing parent-path, unavailable-parent,
cycle and parent/entry-version causes. They retain declaring source, entry,
parent, closed-loop paths and exact version text. Complete context admission
precedes copies; malformed loop carriers and quota refusal remain distinct.
The engine exposes borrowed projection without changing traversal effects or
retaining a second host error. The original-document adapter uses the shared
envelope directly. The legacy inheritance/1 probe still represents the same
core findings in its compatibility context list until API retirement.
Captured schema arenas, resource failures and opaque host errors retain their
separate authority and precedence. This slice contributes to #1753/#1754; it
does not complete global diagnostic or legacy protocol removal.

Inherited-column dependency findings now use the same owned core diagnostic.
Unknown dependencies retain REQ-0070; dependency cycles retain the resolver's
entire unresolved authored column list and REQ-0072, distinct from a closed
parent traversal loop. Whole report admission precedes copied paths/context and
returns an explicit resource refusal without a fitting semantic prefix. The
schema/1 compatibility representation translates these core facts without
assigning conditions or requirements. The regex/1 syntax projection similarly
uses the core registry's existing invalid-pattern cause. Legacy protocol shape
retirement remains part of #1751/#1753.
