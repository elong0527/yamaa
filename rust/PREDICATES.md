# Typed predicate evaluation

`yamaa_core::predicate::Plan` evaluates an explicitly constructed, normalized
predicate arena without host, storage-framework, parser or regex dependencies. It is an
internal building block for filters and checks, not a specification entrypoint.
Python remains the default, and native installations still report
`execution_supported=false`.

## Semantics

The implemented nodes are Boolean constants, NOT, AND, OR, six comparisons,
IS NULL/IS NOT NULL, IN/NOT IN, BETWEEN/NOT BETWEEN and LIKE/NOT LIKE. Operands
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

These policies do not bound allocations made by the caller while constructing
input, time or memory inside a resolver, allocator failure, total work across
repeated runs, or all process memory. No plan constructor or failed run rolls
back resolver effects. A future public transport must add its own admission,
ownership and cancellation guarantees.

## Evidence and remaining integration

Twenty independently authored fixture cases are replayed by the typed Rust
evaluator and the Python parser/evaluator. They pin result/condition identity and
resolution order; expected values are not regenerated from either implementation.
Rust tests additionally exercise the complete truth tables, all comparisons,
large integers, temporal precision, missing/type precedence, opaque errors,
structural expansion, text/work limits and recovery. An independent recursive
matcher checks 7,225 short LIKE combinations against the dynamic program.

This is not predicate syntax compilation. The caller must validate grammar and
literal representations before constructing the plan, including static ESCAPE
errors and literal overflow/temporal diagnostics. The typed interface cannot
represent a portable-regex call; valid `str_contains` must remain explicitly
unsupported at future admission until regex contract 2.0.0 is implemented and
qualified. The shared parser/compiler, source-selection filters,
lookup/window/BMI integration and full Python/R specification execution remain open
gates. Root/row-template filters and assert/implies checks compose this evaluator through the
[dataset/1 bridge](DATASET_TRANSPORT.md), with complete phase-aware binding before
source IPC decoding. Installed Python and R replay the shared typed filter cases;
the optional Python frontend uses the existing parser as a temporary syntax port
and never calls the reference predicate evaluator on native data or declaration
representatives. Check declarations validate nonmissing type representatives
before actual rows, preserving eager evaluation and the completed check ledger
when a later predicate raises a semantic condition.
