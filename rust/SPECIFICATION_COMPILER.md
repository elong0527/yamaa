# Original-specification compiler slice

This internal entry point advances #1739 without changing the default runtime.
It consumes captured raw current-schema modules and original standalone YAML,
normalizes and validates them in Rust, and retains schema snapshots, source bytes
and occurrence origins. Installed Python and R use the same owned representation.
Neither host supplies a semantic model, normalized document or typed dataset plan.

The admitted subset includes CSV/Parquet source profiles, optional declared CSV
types, input-backed named selections, implicit key grain with source/scalar-literal
columns, checked numeric computations and output windows, and closed record/group
templates with source/literal derivations and ordered SUM. The six original
cohort documents pass private installed entry points in both hosts, including
lookup, full output windows and inherited preparation. Non-output helper columns
remain in the internal dataset. Source ordinals, row-output dependencies,
governed submission metadata, other local handlers and
broader expressions remain explicit
Unsupported outcomes before study reads. Transcendental math remains unsupported;
this slice makes no #1740 numerical-policy choice.

Original column literals use the same admitted scalar leaf reader as row literals
and lower to the existing core `Expression::Literal`. Conversion runs at the
existing column boundary, preserving bool/type errors, full i64 values, temporal
conversion, missing and present empty text. Four independent complete reports pin
successful exact CSV and three conversion failures through retained build/save
results. Arbitrary-width integers remain Unsupported before source effects;
recursive expressions and other handler families remain Unsupported.

Preparation scans unsupported vocabulary before study capture. Typed CSV admission
in core then precedes formula diagnostics and source-schema binding.
The source typing service owns declaration admission, field resolution, conversion
order and typed ingestion diagnostics; the physical adapter checks resource bounds
and constructs Arrow storage around that service. Numeric execution
preserves written evaluation order. Unique and row_count checks run after output
keys. A later invalid check declaration retains completed verification records;
it overrides earlier check data failures, while derivation and key failures take
precedence over it. Verification IDs are retained in records and failure context.
Warning severity, other check operations and arbitrary-width count bounds outside
signed 64-bit remain explicitly unsupported.

Output declaration findings occur after successful checks. Shared projection and
bounded R020 CSV rendering preserve exact integers, shortest positional floats,
quoted empty text versus missing, UTF-8, quoting and LF record endings. Complete
CSV bytes and the bounded report are prepared before the host publication call.
Bounded Parquet publication uses the existing closed profile; decimal formatting
remains unsupported at this entry point.
Row-local source/literal column defaults are inherited per template, with overrides
retaining their own paths. Coverage errors, conflicting root filters and aggregate
default conflicts fail before source capture. Grouping fields and scalar group
scope bind after ingestion. Nonnumeric SUM arguments retain REQ-0510 at the authored
aggregate path; all-missing groups remain missing, and source access failures take
precedence over fold errors. Bare SUM operands and operands naming a different
driver retain REQ-0329 after ingestion, before generic field binding.
Row-output dependencies, broader expressions and
remaining phase/default relationships still need shared-compiler work.

## Resource ownership and host boundary

`CapturedSchema` owns a complete named raw module closure. `PreparedRun` retains
its original specification, admitted model and source-independent compiler state.
A source port owns filesystem authorization, immutable capture, cache policy and
verification. Rust supplies the declared name/path and byte ceiling; ports must
bound reads before allocating a reply. The compiler defensively checks bytes and
table shape again, creates an owned lossless Arrow snapshot and binds its schema.
Core CSV profile admission preserves undeclared text/missing values without host
inference; the adapter owns physical Arrow representation.
Declared types use shared scalar conversion in stored row/field order, retaining
full-range integers, float bits and temporal precision.

Each observed execution retains the actual port request, snapshot-counter delta,
captured bytes and admitted source table, including failed binding or execution.
Reporting borrows that same table and never rereads a source. Cached capture is a
request with zero new snapshots. The installed test ports are bounded readers for
fixed approved fixture paths; they do not replace a production resource port's
path/identity/verification obligations.

Python `_prepare_specification` returns a frozen native handle. Its internal
`execute_csv` provides dataset/1 integration results. `report` takes explicit
capture and publication callables and renders complete 0.3.0-draft reports for the
admitted success/failure outcomes; `failure_report` retains its earlier interface.
Capture returns `(bytes, newly_created)` and publication receives `(path, bytes)`.
Publication must return `None`. Original Python exceptions and interruptions are
retained, and a failed publication cannot return a success report.

R exposes `prepare_specification`, `specification_source`,
`specification_failure_report` and `specification_report`. Its wrapper retains
original capture/publication errors and interruptions, rethrowing only after Rust
returns. R external pointers are looked up in a weak registry by identity; unknown
incoming pointees are never cast or dereferenced. Both hosts supply IO authority,
not a semantic model or execution plan. Production atomic publication and resource
verification remain host-port responsibilities.

Compiler/ingestion failures use the explicitly experimental
`specification/prototype` envelope. Semantic diagnostics, Unsupported vocabulary
and resource/boundary rejection are separate outcomes. Detailed schema/capture
failure reporting remains incomplete. The complete-report formatter refuses policy
rejection and unrepresented capture failures instead of inventing observations.
Failed verification/output runs retain the source snapshot only; the reference
report includes derived tables only for successful execution.

The reference observer now retains already evaluated checks when a later
verification declaration raises, matching REQ-1177 and the existing assisted
native path. Independent complete-report regressions cover both an earlier held
check and an earlier violated check; the unreached declaration has no record.

## Evidence and remaining qualification

Independent complete report fixtures cover the unchanged `negative-zero-division`
and `negative-integer-overflow` documents, plus the original successful
`adam-adlb-ordered-sum` report and its unchanged 1,030-byte CSV. Separate reference-only tests verify
that truth; neither runtime implementation regenerates it. Installed tests forbid
Python semantic imports, compare all report fields, exercise cached repeats and
preserve host failures. R's installed source archive owns the same original schema,
YAML and CSV inputs and requires no Python at runtime.

These tests remain supplemental compile/integration evidence pending a complete
host frontend and the fixed-cohort qualification inventory. All six original
#1739 cases, inherited path/version/cycle failures, wheel/source/R platform checks,
production source verification/publication and disconnection of migrated
Python bridge branches remain required. #1739 and parent #1585 stay open.

Row and ordinary-column result wrappers now compile an explicit `unconvertible` literal
into the existing core conversion handler, with its own authored path. Omission
and explicit null remain distinct. Replacements add no dependencies and run only
when conversion fails; missing inputs bypass recovery. Five complete independent
reports pin positive/zero handler counts, exact successful CSV and failed
replacement conversion with retained observations. Eight additional complete reports
pin row-local recovery and inherited defaults: repeated effective defaults share
one counter, overrides keep their authored paths, and a wholly overridden default
has no counter entry. Registration follows effective rows before ordinary columns.
Nested expressions and other handler families remain Unsupported.


Driver-backed source expressions retain optional predicate eligibility in the core
compiled model and bind it only after complete source ingestion. Filter identifiers
are confined to their own input dataset; single output or selected intermediate
values have no record collection to filter. The compiler preserves that normative
rejection before parsing an inapplicable predicate. Valid bindings reuse the
existing core predicate and engine distinct-reading collection services, including
missing/no-match behavior and original source/declaration order. Repeated identifier
occurrences and BETWEEN subjects share one admitted field binding.

The admitted slice includes filtered non-key driver columns. Filtered key
assignments, secondary source expressions and source ordering remain explicit
Unsupported operations before study authority. The
unchanged negative-source-missing-field document now retains independent complete
reference truth on the private route. Thirteen independently authored source-filter
reports preserve five successful cases (three distinct exact CSV outputs) and
eight post-ingestion failures; broader predicate parity gaps remain documented.


`first_available` now compiles driver-backed filtered operands and completed
output references into a core-owned ordered selection. All operand bindings are
admitted before execution, including names that runtime selection will skip.
Execution selects the first present raw value and converts its result once; an
unconvertible selected value does not advance to a later operand. The optional
missing literal records no handler count. Empty lists and omitted/null fallback
retain ordinary missing semantics. Distinct-reading conflicts use the enclosing
expression path; static findings retain their individual operand paths.

Eleven independently authored reports cover six complete successful outputs and
five failures, including skipped conflicting reads, unknown names in skipped
operands and conversion failures. Their exact report/artifact truth was compared
with actual reference Python execution. The unchanged negative-source-trivial-filter
document now reaches its complete REQ-0148 failure through the same core compiler.
Key-phase, row-template, secondary and named-intermediate selection operands
remain outside this bounded slice. The compiler charges all operand/filter
metadata before owned compilation, with a separate trusted source-operand policy.


Original error-severity dataset assertions compile output-column predicates in
core. Unknown names and ordinary grammar findings are deferred to declaration
order after derivation and output keys, retaining the completed check prefix.
All output references are admitted, including names behind inactive guards.
Predicate sample validation precedes actual-row evaluation, even for empty
output or an inactive when clause. A valid when declaration remains a checkpoint
before a later require finding, preserving incompatible-type precedence.

Seventeen independent complete reports cover success, failed checks, null and
inactive guards, empty output, unknown/qualified fields, grammar, sample types
and duplicate identities. Three successful cases retain exact CSV bytes. Both
assertion predicates share the compiler's aggregate text budget before owned
compilation. Warning severity, column verifications, nontext predicates and the
existing special predicate requirement/context gaps remain unsupported.


Original error-severity all_or_none checks bind declared output columns and
validate at least two distinct names in declaration order. Repeated names are
permitted. Each row must have every named value present or every value missing;
empty output holds. Completed checks and full offending-key logs survive failures,
while diagnostics retain their existing shorter key samples. All declared cell
reads share the engine work budget, including repeated references.

Eleven independent complete reports and three exact successful CSV cases cover
these behaviors and invalid declarations. The unchanged negative-paired-dates
benchmark separately retains complete independently checked truth over its
declared date source cells. Warning severity and column verifications remain
unsupported; no public frontend or assisted-inventory promotion is claimed.
