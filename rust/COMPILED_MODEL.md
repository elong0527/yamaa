# Shared compiled-model ownership

The first #1752 prerequisite places immutable expression bindings and function
signatures in `yamaa-core`. The engine consumes these exact types through
temporary reexports; there is no second representation or translation step.

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

The engine retains table resolution, resource counters, function callbacks,
failure payloads, handler accounting and execution order. Its
`function_invocation::invoke` application operation now accepts the core-owned
signature. A reused plan still repeats every authored read and host call; moving
admission does not cache effects, fold arithmetic or evaluate failing literals.

Core tests exercise name completeness, scope/error precedence, deferred numeric
failures, signature admission, declaration order and explicit missing defaults.
The existing engine and installed-host suites continue to exercise effect order,
budgets, exact outcomes and opaque host failures through the same declarations.

This prerequisite does not complete #1752. `DatasetPlan`, its remaining
declarations/admission, and the existing engine `PreparedSpecification` compiler
still need to move into the shared immutable model. Source/inheritance provenance
and the full six-document cohort remain required. Adapters still assemble some
typed probe plans; #1754 retires those semantic responsibilities as domain APIs
replace the probes. No public API cutover, benchmark inventory promotion or
numerical-policy change is included here.
