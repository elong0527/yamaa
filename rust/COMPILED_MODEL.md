# Shared compiled-model ownership

The #1752 foundation places immutable expression bindings, function signatures,
the dataset plan and the existing original-document compiler in `yamaa-core`.
The engine consumes these exact types through temporary reexports; there is no
second representation or translation step.

`bound_expression::BoundNumeric` and `BoundPredicate` own their compiled trees
and complete, unique name maps. Pure admission sorts names, checks every
identifier (including predicate branches that can short-circuit), and validates
source/output availability at each use site. Grouped-source restrictions and
first-error order are preserved. Borrowed accessors expose admitted state without
allowing callers to mutate a plan after validation.

`function_signature::InvocationPlan` owns the resolved identity, ordered logical
parameters, host-name mapping, defaults and result contract. Admission rejects
invalid names, duplicate mappings, invalid defaults and empty identity fields
without activating or invoking a function. `conversion::LiteralHandler` owns a
declared replacement value and its specification path.

`dataset::DatasetPlan` owns the complete admitted declaration graph: source and
output schemas, row templates, column assignments, keys, checks, windows,
lookups, intermediate selections and conversion handlers. Its fields are private.
Construction validates all references and phase dependencies before any table
access; borrowed accessors expose only admitted state. Handler lookup uses the
admitted assignment path without exposing its internal index.

`specification::PreparedSpecification` compiles the existing supported subset of
normalized original documents. Preparation inspects the admitted document;
binding consumes actual captured source schemas in authored input order after
ingestion. The driver index is independent of that order. Input-backed named
selections compile filters from original predicate text, match keys from completed
output dependencies, and record ordering/absence handling into the same core
`Intermediate` declarations used by the dataset engine. Both steps
now run entirely in core. Unsupported declarations, resource failures, deferred
formula diagnostics and output checks retain their existing boundaries.

The engine retains table resolution, resource counters, function callbacks,
failure payloads, handler accounting and execution order. Its
`function_invocation::invoke` application operation now accepts the core-owned
signature. A reused plan still repeats every authored read and host call; moving
admission does not cache effects, fold arithmetic or evaluate failing literals.
The engine's `dataset::DatasetExecution` trait runs the core plan using a private
borrowed executor. The executor stores only a plan reference; budgets, source
ports, callbacks and handler ledgers remain local to each attempt.

Core tests exercise name completeness, scope/error precedence, deferred numeric
failures, signature admission, declaration order and explicit missing defaults.
The existing engine and installed-host suites continue to exercise effect order,
budgets, exact outcomes and opaque host failures through the same declarations.
Dataset admission tests now run directly in core. Core compiler tests prepare
and bind a normalized document without linking the engine, including deferred
division by zero, formula syntax findings and unsupported source formats.

The temporary `dataset/1` codec now decodes typed declarations with no execution
interface. Core's pure `compile_dataset` service accepts `DatasetDeclaration`;
its `DatasetCompiler` draft also accepts those declarations in authored order.
These services compile numerical text, bind supplied names, resolve function
slots, enforce one cumulative signature-copy budget across row and column
phases, classify the legacy number/value-window forms, and admit the complete
dependency graph and conversion handlers. Only final admission returns an
immutable `CompiledDataset` containing the same `DatasetPlan` used by original YAML.
An adapter cannot publish a partly admitted plan or construct an executable plan
directly. JSON parsing, lossless scalar/decimal codecs and transport bounds remain
in adapters; the existing core predicate/signature component services admit their
typed carriers. Neither declaration decoding nor compilation activates functions,
reads table cells or evaluates arithmetic.

The codec feeds each decoded assignment to the pure draft in its original
admission order. Core grammar and function-slot admission occur before subordinate
binding/scalar codecs, and expression faults precede later verification, handler
and intermediate payload faults. Wire-wide quotas still run first. Window-form
admission likewise precedes subordinate payload limits. The draft exposes no
executable plan; any failed expression poisons it, and unused numeric/function
reservations prevent final admission. Mixed-fault regressions pin the existing
temporary protocol's `InvalidPlan`/`RequestLimit` precedence before bad IPC.

Original-YAML preparation retains the exact entry and parent bytes, a location
for each raw occurrence, normalized node origins and inherited written-layer
provenance alongside the compiled model in one checked application handle.
Compiler findings retain authored paths; those paths resolve against the held
document's declaring source and raw locations without another read. The original
compiler resolves declarations, operation vocabulary and dependency phases at
preparation. Source-field binding uses captured immutable schemas at ingestion,
because undeclared CSV fields and physical Parquet types are not known earlier.
Unsupported operations fail during preparation, before study inspection,
capture, decoding or any callback activation. Arithmetic and value-dependent
checks retain their required execution timing.

`yamaa-adapters/tests/compiled_model.rs` prepares the six unchanged #1739 YAML
documents and binds independently enumerated source schemas without study data.
It checks keys, output/declaration order, row phases, ordered SUM, all nine window
operations, lookup match coordinates, inheritance pruning and written origins.
A changed inherited source profile is refused with its authored path, parent
identity and retained raw location before study effects. Pure core regressions
also cover numeric grammar/binding/phase rejection, cumulative function-copy
budgets, legacy window vocabulary and atomic recovery admission. Existing
complete-report fixtures supply independent value/type/order/CSV and failure
observations for these same documents.

The bounded public six-document qualification completed in
[#1813](https://github.com/elong0527/yamaa/pull/1813), closing #1739 with 96 installed
complete-run tuples on its applicable Python/R matrix. The twelve Windows R
tuples are explicitly separate #1903 work. This compiled-model gate does not
claim full-language coverage, temporary protocol retirement, numerical-policy
changes or completion of that remaining platform qualification.

Original output-window declarations now compile into core `Window` values after
shared schema admission expands named windows. Column binding records the actual
source, grouping, ordering and predicate dependencies; the existing engine owns
partition execution. The unchanged window fixture covers all 13 rows and the
complete CSV through installed hosts. Field-specific validation and baseline
ambiguity reports retain their observed source tables and partition identity.
Qualified window reads, row-template windows and broader baseline type failures
remain outside this bounded compiler slice; they are not full-language coverage.

An inherited-document preparation service now sequences the existing traversal,
composition, named-window expansion, reachability pruning and final admission
operations in the engine. Canonicalization, reads and lexical path rebasing use
explicit host ports. The retained result owns normalized contributions, written
layer provenance, expansion history and the document addressed by final origins.
A direct integration test prepares the unchanged `spec_study.yaml`, reads its
shared parent once, prunes the unused input and column, and produces the exact
committed CSV. The adapter retains raw parent bytes and locations and compares
the complete report against independent reference-checked truth. Installed Python
and R routes now exercise all six original documents, including exact parent
callback errors/interruptions and cached source capture. The existing conformance
frontend uses this owned lifecycle and compares complete public cohort reports.
Non-governed descriptive metadata can reach compilation; reserved metadata keys
remain explicitly unsupported pending submission validation integration.

Scalar literal leaves now share one core reader across row and column declarations.
Ordinary columns lower directly to the existing immutable `Expression::Literal`,
with no host plan assembly or preparation-time conversion. The engine retains
column conversion, full diagnostic context, completed observations and explicit
save behavior. Independent complete reports pin successful publication bytes and
three conversion failures; arbitrary-width integers remain Unsupported before
source effects.

Ordinary-column `unconvertible` wrappers now add core `ConversionHandler` values
to the same immutable plan, preserving declaration order and distinct assignment
and handler paths. Existing engine recovery owns triggering, replacement
conversion and positive/zero ledger counts. Explicit null is retained as a
present handler; an omitted handler remains absent. No host assembles these plans
or selects recovery behavior.

Row wrappers use the same scalar-handler admission as ordinary columns. The row
compiler binds only effective declarations, in row order before ordinary columns;
repeated inherited defaults share a single handler and counter. Completely
overridden defaults are admitted structurally but do not register unused handlers.
Eight independent complete-report cases pin those rules without engine changes.
