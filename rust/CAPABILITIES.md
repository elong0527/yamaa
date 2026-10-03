# Rust migration capability coverage

This matrix tracks the scalar slices of [#1585](https://github.com/elong0527/yamaa/issues/1585).
Core tests are not dataset execution. Every native installation probe still
reports `execution_supported = false`; all 315 benchmark cases remain executable
through the Python backend only, as declared by
[`execution-manifest.yaml`](../benchmarks/execution-manifest.yaml).

| Contract / requirements | Rust core coverage | Python / Rust backend | R / Rust backend | Remaining gate |
| --- | --- | --- | --- | --- |
| Closed values: REQ-0002, 0006-0007, 0014 | Unit tests: distinct bool/column types, missing/absent selection, UTF-8 strings, full i64, finite binary64 wrapper | No dataset execution | No dataset execution | Host normalization, typed tables, source and function boundaries (#1585 steps 5-8) |
| Comparison: REQ-0004-0005, 0324 | Unit tests: present text, numeric and temporal values; exact mixed comparisons at large i64 values | No dataset execution | No dataset execution | Predicate grammar, operation-specific diagnostics, missing ordering policies (steps 4, 7, 9) |
| Basic arithmetic: REQ-0420-0421, 0424 (ABS/MOD only), 0427, 0430, 0434 | Shared hand-written vectors replayed by Rust primitives and Python evaluator; overflow, division, types, missingness, float bits | No dataset execution | No dataset execution | Remaining functions and rounding; parser and handler integration (steps 4, 7) |
| Typed numeric evaluation: REQ-0426-0427, 0429, 0438, 0443-0444 | Typed literal/identifier/unary/binary nodes, fake resolver port, ordered evaluation, exact failure provenance; 22 shared evaluation/trace vectors plus 49 arithmetic cases through the plan | No dataset execution | No dataset execution | Remaining math/rounding execution, host diagnostic transport and lifecycle handlers (steps 4, 7) |
| Numeric syntax: REQ-0413-0415, 0439-0441 | Bounded arena parser; all 44 shared grammar cases, closed function/arity/reserved-word checks, diagnostic positions and deterministic Python comparisons | No dataset execution | No dataset execution | Full function execution and lifecycle diagnostics (steps 4, 7) |
| Numeric compilation: REQ-0413-0415, 0426-0427, 0434, 0438-0444 | Immutable supported-subset plans, deferred exact literal failures, source spans, unsupported preflight and static resolution budgets; 22 literal cases plus arithmetic/trace fixtures through compilation | No dataset execution | No dataset execution | Remaining functions, completed-result conversion and handlers (steps 4, 7) |
| Numeric selection: REQ-0415-0416, 0424-0427 | GREATEST, LEAST, NULLIF and COALESCE in primitives/typed evaluation/compilation; 51 shared value, promotion, missingness and eager-resolution cases | No dataset execution | No dataset execution | Remaining math/rounding functions, completed-result conversion and handlers (steps 4, 7) |
| Integral-valued numeric functions: REQ-0415, 0424, 0426-0427 | CEIL, FLOOR and TRUNC in primitives/typed evaluation/compilation; 100 shared cases with exact float bits, full-range i64, subnormals, signed zero and failures; standard-library differential tests across every finite exponent | No dataset execution | No dataset execution | Remaining transcendental/rounding functions, completed-result conversion and handlers (steps 4, 7) |
| Square root: REQ-0424, 0426-0427, 0431 | SQRT primitive, typed evaluation and compilation; 32 shared exact-value/domain/failure cases and 110,188 bit-exact Python comparisons in native CI; pinned no_std libm | No dataset execution | No dataset execution | EXP/LN/POWER and decimal rounding, completed-result conversion and handlers (steps 4, 7) |
| Remaining math compatibility: EXP/LN/POWER | Candidate-only probe and per-platform JSON observations; exact differences block qualification, even at one ULP; compiler still returns Unsupported | No dataset execution | No dataset execution | Choose and qualify explicit shared numerical behavior; include domain/zero/missing and dependent rounding/output contracts before enabling (steps 4, 11) |
| Temporal values: REQ-0539-0555, 0559-0561, 0567-0573 | Validated civil fields, strict parsing, canonical text, precision, equality and chronological order; 400-year calendar cycle | No dataset execution | No dataset execution | Imputation, temporal operations, function/artifact boundaries (steps 4, 5, 8, 9) |
| Conversion: REQ-0009-0013, 0015-0018, 0020-0021, 0601 | Full scalar matrix, strict numeric text, exact range/integrality checks, canonical numeric/temporal text, structured failures; shared reference vectors and deterministic float-text differential check | No dataset execution | No dataset execution | Integration at the completed-result lifecycle boundary and actual handler application (steps 4, 7, 8) |
| Regex and remaining scalar grammars | Not implemented | No dataset execution | No dataset execution | Replay existing grammar/regex vectors, including backreferences and lookarounds (step 4) |
| Application/workflow, tables, verification, publication, callbacks | Not implemented | No dataset execution | No dataset execution | Fake-port application tests; Arrow and FFI tests; bounded vertical prototype (steps 5-10) |

`compare_present` mirrors the current Python value comparator: it does not round
an i64 through binary64 to decide ordering. Arithmetic promotion intentionally
does widen an integer when an operand is float. Boolean values are not accepted
by the ordered-value comparator, matching the existing Python implementation;
predicate truth/equality is a separate, unimplemented interface. Missing placement
is likewise owned by the consuming operation, not silently chosen by this API.

Arithmetic primitives still take already-evaluated `Number` values. `NumericPlan`
now evaluates a caller-supplied typed tree for literals, identifiers, unary signs,
ABS, arithmetic operators and MOD. It resolves each occurrence left-to-right,
stops at the first failure, and evaluates both operands before missing
propagation. A missing left operand therefore does not suppress a failure in the
right subtree. Tree association is preserved and values are never memoized.

A core-owned resolver port returns normalized core values, absence, or its own
failure payload. Nonnumeric values produce REQ-0444 without implicit conversion;
absence produces REQ-0443 rather than missing. Resolver failures remain opaque
and intact, including data needed by later lifecycle handling. Numeric failures
retain condition, phase, requirement, exact arithmetic overflow, original text,
specification path and the structural operand route. The route is not a source
character span. No handler is selected, applied or counted by this evaluator.

This evaluator remains an internal typed IR, not a public host API. A separate
`numeric_parser` now accepts the full numeric syntax, preserves literal spelling
without converting it, and records half-open UTF-8 spans plus Unicode scalar error
offsets. Its immutable arena is protected by byte/token/node/depth budgets; depth
is capped at 64 even when a caller requests more. Resource exhaustion is separate
from grammar conditions. The parser handles every allowed function syntactically;
that does not claim execution support for those functions.

`numeric_compiler` now connects source text to immutable plans for the supported
arithmetic subset. It reports all valid unsupported calls in source order before
execution, retains runtime failure spans and enforces a static resolution budget
alongside the parser's structural limits. Every identifier occurrence counts,
including repeated names, even if an earlier failure would prevent reaching it.
The budget applies independently to each evaluation and does not bound time spent
inside a resolver. The caller-supplied `NumericPlan` API itself remains unbounded.
Remaining math/rounding functions, host diagnostic transport and lifecycle
integration remain outstanding.

Selection functions eagerly evaluate every argument before applying missingness,
selection and promotion. Variadic failures use zero-based argument routes and
retain the exact inner failure span. Extrema compare present values exactly before
promoting the selected result; NULLIF preserves the reference's comparison after
binary64 promotion for mixed pairs. All-int comparisons remain exact.

Python COALESCE previously returned the first present value without the REQ-0424
promotion required when any present argument is float. This slice explicitly fixes
that reference discrepancy alongside Rust support: the shared 51-case corpus pins
result types, float bits, resolver traces and downstream arithmetic. Missing
arguments do not force promotion. The correction may change large-integer results
or completed-result conversion and is not described as behavior-neutral parity.

CEIL/FLOOR/TRUNC always produce float for present input and canonicalize zero
results to positive zero, including negative-zero inputs and negative fractions
rounded to zero. Missing propagates after operand evaluation. The implementation
uses finite binary64 bits without a new dependency or bounded integer intermediate;
it preserves very large integral floats. The 100 shared vectors compare exact bits,
resolution traces and failure routes; Rust additionally compares standard-library
results at exponent/significand boundaries and deterministic finite bit patterns.

SQRT uses pinned libm 0.2.16 with default features disabled and an explicit dependency
allowlist entry. Negative finite input yields `sqrt_of_negative` (REQ-0431), while
negative zero remains a valid signed result. Integer input promotes before square
root; missing propagates. Source/operand order and static budgets use the existing
compiler. Exact Python math.sqrt comparisons run on each native Python CI target;
there is no blanket numerical tolerance. Other libm functions are not yet enabled
or claimed compatible, and no host/dataset backend is exposed by this slice.

Python numeric-expression integer literals now also avoid the host's integer
string digit limit: insignificant zeros are stripped before bounded construction,
and oversized positive literals report exact decimal text under REQ-0434. This is
a separate path from completed-result conversion. Overflow stays at evaluation
time in written order, before a literal's unary sign. Rust compilation now retains
that overflow as a deferred node instead of failing eagerly. Both implementations
replay 22 shared literal expectations, including signed i64 boundaries, 5,000-digit
inputs, decimal/exponent typing and nonfinite normalization. No integer literal is
narrowed through binary64.

Conversion failures retain the parsed source type/value used by Python, the
destination, phase, condition, eligible handler and owning requirement. An
out-of-range integer is retained as canonical decimal diagnostic text, never a
successful runtime value. Error transport/serialization and handler application
remain separate integration work. A decimal/exponent numeric spelling parses as
binary64 before integral conversion; plain integer spelling does not lose i64
precision by passing through a float.

Long integer text follows REQ-0015 without inheriting Python's configurable
integer-string digit limit. Both runners replay 105 shared conversion vectors,
including 5,000-digit inputs, leading zeros, signed i64 boundaries, exact large
integers, malformed text and overflow. Python strips insignificant zeros before
bounded integer construction. Diagnostics for more than 19 significant integer
digits retain canonical decimal text with source type `int`; shorter overflow
values keep their existing JSON numeric representation. This deliberate change
to oversized diagnostic values avoids unbounded integer construction and keeps
JSON/error-log serialization independent of the host's digit limit. Such values
never become successful runtime integers. Rust already retains all out-of-range
integer diagnostics as canonical text internally; its eventual host transport
must preserve this distinction.

The checked-in TSV contains explicit expected results, including raw binary64
bits (not digests). Both test runners compare to those expectations; neither
writes them. Temporal tests separately exercise strict lexical forms, leap
centuries, zone rejection, precision retention and loss on text round trips.

No new native API, default backend, benchmark manifest entry, or golden artifact
changes in this slice. Build qualification and the outstanding Cargo-lockfile
policy / R compiled-code warning are recorded in [`README.md`](README.md).

The candidate math assessment is not a new executable capability. On the local
macOS arm64/Python 3.14.7 sample, libm 0.2.16 differs in 989 EXP, 279 LN and 950
POWER results out of 10,011 inputs each (one ULP per observed difference). Native
CI retains complete per-platform observations as `math-assessment-*` artifacts.
Even a zero-mismatch sample is labeled `not-qualified`; functions remain explicitly
unsupported until a shared numerical policy is implemented and independently
qualified. Existing SQRT bit-exact checks remain strict and are not relaxed.
