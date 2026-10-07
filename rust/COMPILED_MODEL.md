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
binding consumes the actual captured source schema after ingestion. Both steps
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
