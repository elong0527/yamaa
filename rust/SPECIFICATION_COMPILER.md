# Original-specification compiler slice

This internal entry point advances #1739 without changing the default runtime.
It consumes captured raw current-schema modules and original standalone YAML,
normalizes and validates them in Rust, and retains schema snapshots, source bytes
and occurrence origins. Installed Python and R use the same owned representation.
Neither host supplies a semantic model, normalized document or typed dataset plan.

The admitted execution subset is one untyped CSV driver, implicit key grain,
source columns and checked numeric computations. Non-output helper columns remain
in the internal dataset. Declared source types/ordinals, row templates, metadata,
handlers, checks, lookups, windows and inheritance require later slices and are
explicitly rejected before study reads. Transcendental math remains unsupported;
this slice makes no #1740 numerical-policy choice.

Preparation collects preflight findings before study capture. CSV admission then
precedes formula diagnostics and source-schema binding. Binding retains all
findings in authored column order, followed by dependency diagnostics. Numeric
execution preserves written evaluation order. Successful output projection,
formatting, verification and publication are not implemented by this entry point;
output declaration validation must eventually occur after derivation, preserving
the reference error order.

## Resource ownership and host boundary

`CapturedSchema` owns a complete named raw module closure. `PreparedRun` retains
its original specification, admitted model and source-independent compiler state.
A source port owns filesystem authorization, immutable capture, cache policy and
verification. Rust supplies the declared name/path and byte ceiling; ports must
bound reads before allocating a reply. The compiler defensively checks bytes and
table shape again, creates an owned lossless Arrow snapshot and binds its schema.
The CSV profile preserves exact text and missing values without host type inference.

Each observed execution retains the actual port request, snapshot-counter delta,
captured bytes and admitted source table, including failed binding or execution.
Reporting borrows that same table and never rereads a source. Cached capture is a
request with zero new snapshots. The installed test ports are bounded readers for
fixed approved fixture paths; they do not replace a production resource port's
path/identity/verification obligations.

Python `_prepare_specification` returns a frozen native handle. Its internal
`execute_csv` provides dataset/1 integration results. `failure_report` invokes an
explicit capture callable returning `(bytes, newly_created)` and renders a complete
0.3.0-draft failure report. Original Python callback exceptions are retained.
R exposes `prepare_specification`, `specification_source` and
`specification_failure_report`. Its wrapper retains original errors/interrupts and
rethrows them only after Rust returns. R external pointers are looked up in a weak
registry by identity; unknown incoming pointees are never cast or dereferenced.

Compiler/ingestion failures use the explicitly experimental
`specification/prototype` envelope. Semantic diagnostics, Unsupported vocabulary
and resource/boundary rejection are separate outcomes. Detailed schema/capture
failure reporting remains incomplete. The complete-report formatter refuses
success, policy rejection and unrepresented capture failures rather than creating
accepted artifacts or invented observations. It currently represents text/missing
source snapshots and supported scalar diagnostic contexts only.

## Evidence and remaining qualification

Independent complete report fixtures cover the unchanged `negative-zero-division`
and `negative-integer-overflow` documents. Separate reference-only tests verify
that truth; neither runtime implementation regenerates it. Installed tests forbid
Python semantic imports, compare all report fields, exercise cached repeats and
preserve host failures. R's installed source archive owns the same original schema,
YAML and CSV inputs and requires no Python at runtime.

These tests remain supplemental compile/integration evidence pending a complete
host frontend and the fixed-cohort qualification inventory. All six original
#1739 cases, inherited path/version/cycle failures, wheel/source/R platform checks,
source verification, successful artifact publication and disconnection of migrated
Python bridge branches remain required. #1739 and parent #1585 stay open.
