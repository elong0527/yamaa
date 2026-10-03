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
| Typed numeric evaluation: REQ-0426-0427, 0429, 0438, 0443-0444 | Typed literal/identifier/unary/binary nodes, fake resolver port, ordered evaluation, exact failure provenance; 22 shared evaluation/trace vectors plus 49 arithmetic cases through the plan | No dataset execution | No dataset execution | Numeric text parser, full function vocabulary, source character spans, compiler resource budgets and handlers (steps 4, 7) |
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

This is an internal typed IR, not a numeric text parser or a public host API.
The recursive tree/evaluator expects compiler-owned inputs; depth/resource
budgets must be defined before compiling or exposing untrusted expressions.
Literal lexical validation, full function vocabulary, source character spans,
diagnostic transport and lifecycle integration remain outstanding.

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
