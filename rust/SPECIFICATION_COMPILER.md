# Original-specification compiler slice

This internal entry point advances #1739 without changing the default runtime.
It consumes captured raw current-schema modules and original standalone YAML,
normalizes and validates them in Rust, and retains schema snapshots, source bytes
and occurrence origins. Installed Python and R use the same owned representation.
Neither host supplies a semantic model, normalized document or typed dataset plan.

The admitted execution subset is one CSV driver with optional declared source
types, implicit key grain with checked numeric computations, or a closed set of
record/group templates with source/literal derivations and ordered SUM. The
original `adam-adlb-ordered-sum` document keeps its column and row phases and
executes all 17 rows. Non-output helper columns remain in the internal dataset.
Source ordinals, row-output dependencies, metadata, handlers, lookups, windows and
inheritance still require later slices and are explicitly rejected before study
reads. Transcendental math remains unsupported; this slice makes no #1740
numerical-policy choice.

Preparation scans unsupported vocabulary before study capture. Typed CSV admission
then precedes formula diagnostics and source-schema binding. Numeric execution
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
Parquet publication and decimal formatting remain unsupported at this entry point.
Row-local source/literal column defaults are inherited per template, with overrides
retaining their own paths. Coverage errors, conflicting root filters and aggregate
default conflicts fail before source capture. Grouping fields and scalar group
scope bind after ingestion. Nonnumeric SUM arguments retain REQ-0510 at the authored
aggregate path; all-missing groups remain missing, and source access failures take
precedence over fold errors. Row-output dependencies, broader expressions and
remaining phase/default relationships still need shared-compiler work.

## Resource ownership and host boundary

`CapturedSchema` owns a complete named raw module closure. `PreparedRun` retains
its original specification, admitted model and source-independent compiler state.
A source port owns filesystem authorization, immutable capture, cache policy and
verification. Rust supplies the declared name/path and byte ceiling; ports must
bound reads before allocating a reply. The compiler defensively checks bytes and
table shape again, creates an owned lossless Arrow snapshot and binds its schema.
The CSV profile preserves undeclared text/missing values without host inference.
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
