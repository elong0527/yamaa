# Typed predicate evaluation

`yamaa_core::predicate::Plan` evaluates an explicitly constructed, normalized
predicate arena without host or storage-framework dependencies. Portable regex compilation and
matching use the shared core. It is an
internal building block for filters and checks, not a specification entrypoint.
Python remains the default, and native installations still report
`execution_supported=false`.

## Semantics

The implemented nodes are Boolean constants, NOT, AND, OR, six comparisons,
IS NULL/IS NOT NULL, IN/NOT IN, BETWEEN/NOT BETWEEN, LIKE/NOT LIKE, and literal-pattern `Contains`
(the typed form of `str_contains`). Operands
are normalized core values or deferred identifiers. Three-valued truth follows
[REQ-0159 and REQ-0171](../rules/operations/predicates.md). Missing comparison
operands produce unknown before type compatibility is checked. Boolean runtime
values remain incompatible with ordered comparison; they are distinct from
predicate truth.

Mixed int/float predicates promote both operands to binary64 under REQ-0167.
Thus integer 9007199254740993 compares equal to float 9007199254740992. Two
integers remain exact. This deliberately differs from `value::compare_present`,
which retains exact mixed-number ordering for table consumers. Text compares
by Unicode scalar order without normalization, and temporal comparison ignores
collected precision while retaining complete civil fields.

Resolution is observable and is never memoized. AND and OR evaluate both
children unless the first fails. IN resolves its subject once, then checks each
item in written order, including after a match. BETWEEN resolves subject, lower
endpoint, subject again, upper endpoint before either comparison. An upper
endpoint resolution error therefore precedes an incompatible lower comparison.
The second subject occurrence has its own structural route.

LIKE matches the complete string. Percent matches any sequence, underscore one
Unicode scalar (including newline), and backslash has no default escape meaning.
An explicit escape is one Rust `char`; a dangling dynamic escape returns
`invalid_predicate` under REQ-0191. Matching uses bounded dynamic programming,
with storage proportional to the pattern, and no host regex dialect.

`Contains` searches anywhere, returns unknown for missing subjects (also under
NOT), and reports a present non-string under REQ-1244. It resolves the subject
once. Every literal, including unreachable arena entries, is compiled during
plan admission. All literals in one plan share the regex compiler's width-work
and logical-storage budgets, while each retains its ordinary compiler limits.
The immutable plan owns compiled patterns; evaluation never compiles a pattern.

An absent binding returns `unknown_field` under REQ-0189. A resolver's own error
moves through unchanged and needs no Clone implementation. Predicate conditions
retain their phase, requirement and typed context. Errors carry the original
expression, specification path and structural route. No source character span,
lifecycle handler, verification record or accepted dataset is invented here.

## Admission and policies

The plan owns a flat arena whose Boolean child indexes must refer to earlier
nodes. All nodes are checked before resolution, even when a failure would make
them unreachable. Empty membership lists and empty identifiers are invalid typed
plans. There is no recursive ownership chain to overflow while dropping a plan.

Caller-selected limits bound arena nodes, expression/provenance/operand text,
expanded evaluation work and resolver occurrences. Depth has a hard ceiling of
64. Shared child indexes count each expanded visit; a small arena cannot hide an
exponential evaluation. BETWEEN counts both subject occurrences. Default limits
allow 4,096 nodes and resolutions, 16,384 expanded node/scalar visits, and
1,048,576 bytes of admitted text.

Each run separately limits cumulative scalar text processing and LIKE work.
Literal text is charged before local cloning; owning resolver text is charged after the
port returns ownership. A resolver may instead lend `ValueRef` through
`resolve_value`; the core charges borrowed text before making an operand copy. LIKE charges pattern tokenization, initial matching cells
and each source-scalar/matching-cell visit before performing that work. Counters
use checked addition. `evaluate` starts a fresh budget; `evaluate_with_budget`
also consumes an application-owned cumulative `Budget`, including on failures.
Dataset filters and checks share that budget across all templates, candidates,
declaration representatives and output rows. Resource
refusals remain separate from language conditions and unknown/false truth.

Regex matching charges two scopes before work/allocation: all Contains visits
in one predicate evaluation and all predicate visits in the dataset attempt.
The local defaults are 1,048,576 subject bytes, 1,000,000 work units and 1,000,000
logical state cells. The dataset/1 bridge gives matching separate cumulative
ceilings of 16 MiB subject bytes and 4,194,304 work units/state cells, shared
across templates, rows, source selections, windows, donors and checks. These
regex counters are independent of ordinary predicate/table work and text
counters. Charges survive no-match results and failed attempts; only a new
attempt receives fresh counters. Refusals retain the resource and original
scope limit, and cannot become false/unknown or an invalid-predicate diagnostic.

These policies do not bound allocations made by the caller while constructing
input, time or memory inside a resolver, allocator failure, total work across
repeated runs, or all process memory. No plan constructor or failed run rolls
back resolver effects. A future public transport must add its own admission,
ownership and cancellation guarantees.

## Evidence and remaining integration

Twenty independently authored fixture cases are replayed by the typed Rust
evaluator and the Python parser/evaluator. They pin result/condition identity and
resolution order; expected values are not regenerated from either implementation.
Eight new typed consumer fixtures cover row/root/source/window/donor filters,
NOT with missing data, assert checks with and without `when`, and eager non-string failure.
Both installed hosts replay exact outcome/snapshot JSON from these authored
inputs; the optional Python frontend separately pins CSVs and callback traces
with reference predicate parsing/evaluation disabled.

Rust tests additionally exercise the complete truth tables, all comparisons,
large integers, temporal precision, missing/type precedence, opaque errors,
structural expansion, text/work limits and recovery. An independent recursive
matcher checks 7,225 short LIKE combinations against the dynamic program.

The separate [shared predicate syntax service](PREDICATE_SYNTAX.md) admits R004
syntax and regex literals without enabling evaluation. The caller of this typed
evaluator must still validate grammar and
literal representations before constructing the plan, including static ESCAPE
errors and literal overflow/temporal diagnostics. The typed interface accepts a closed Contains node with one scalar subject and
one literal pattern string. Invalid patterns are invalid typed plans at this
boundary; compiler resource refusals remain request limits. The syntax service
provides language diagnostics before typed lowering.
The shared parser/compiler, complete source selection and full Python/R
specification execution remain release gates. Root/row-template/source filters,
named donor/window filters and assert checks compose this evaluator
through the [dataset/1 bridge](DATASET_TRANSPORT.md), with phase-aware binding
before source IPC decoding. Both installed hosts replay authored typed consumer
truth. The optional Python frontend uses the captured Rust syntax service and
never calls the reference predicate evaluator on data or declaration samples.
Check declarations validate nonmissing type representatives
before actual rows, preserving eager evaluation and the completed check ledger
when a later predicate raises a semantic condition.
