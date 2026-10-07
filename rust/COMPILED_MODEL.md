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

This foundation does not complete #1752. Source/inheritance provenance, broader
compiler coverage and the full six-document cohort remain required. Adapters still assemble some
typed probe plans; #1754 retires those semantic responsibilities as domain APIs
replace the probes. No public API cutover, benchmark inventory promotion or
numerical-policy change is included here.

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
committed CSV. This is not yet installed-host or complete-report qualification.
Non-governed descriptive metadata can reach compilation; reserved metadata keys
remain explicitly unsupported pending submission validation integration.
