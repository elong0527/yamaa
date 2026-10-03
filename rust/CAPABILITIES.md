# Rust migration capability coverage

This matrix tracks the first scalar slice of [#1585](https://github.com/elong0527/yamaa/issues/1585).
Core tests are not dataset execution. Every native installation probe still
reports `execution_supported = false`; all 315 benchmark cases remain executable
through the Python backend only, as declared by
[`execution-manifest.yaml`](../benchmarks/execution-manifest.yaml).

| Contract / requirements | Rust core coverage | Python / Rust backend | R / Rust backend | Remaining gate |
| --- | --- | --- | --- | --- |
| Closed values: REQ-0002, 0006-0007, 0014 | Unit tests: distinct bool/column types, missing/absent selection, UTF-8 strings, full i64, finite binary64 wrapper | No dataset execution | No dataset execution | Host normalization, typed tables, source and function boundaries (#1585 steps 5-8) |
| Comparison: REQ-0004-0005, 0324 | Unit tests: present text, numeric and temporal values; exact mixed comparisons at large i64 values | No dataset execution | No dataset execution | Predicate grammar, operation-specific diagnostics, missing ordering policies (steps 4, 7, 9) |
| Basic arithmetic: REQ-0420-0421, 0424 (ABS/MOD only), 0427, 0430, 0434 | Shared hand-written vectors replayed by Rust primitives and Python evaluator; overflow, division, types, missingness, float bits | No dataset execution | No dataset execution | Parser/typed IR, operand resolution, source paths, handlers; remaining functions and rounding (steps 4, 7) |
| Temporal values: REQ-0539-0555, 0559-0561, 0567-0573 | Validated civil fields, strict parsing, canonical text, precision, equality and chronological order; 400-year calendar cycle | No dataset execution | No dataset execution | Imputation, temporal operations, function/artifact boundaries (steps 4, 5, 8, 9) |
| Conversion: REQ-0009-0013, 0015-0018, 0020-0021 | Not implemented; temporal parsing/rendering is only a primitive | No dataset execution | No dataset execution | Complete conversion matrix and canonical numeric text (step 4) |
| Regex and remaining scalar grammars | Not implemented | No dataset execution | No dataset execution | Replay existing grammar/regex vectors, including backreferences and lookarounds (step 4) |
| Application/workflow, tables, verification, publication, callbacks | Not implemented | No dataset execution | No dataset execution | Fake-port application tests; Arrow and FFI tests; bounded vertical prototype (steps 5-10) |

`compare_present` mirrors the current Python value comparator: it does not round
an i64 through binary64 to decide ordering. Arithmetic promotion intentionally
does widen an integer when an operand is float. Boolean values are not accepted
by the ordered-value comparator, matching the existing Python implementation;
predicate truth/equality is a separate, unimplemented interface. Missing placement
is likewise owned by the consuming operation, not silently chosen by this API.

Arithmetic inputs are already evaluated `Number` values. Propagating missing in
these primitives makes no claim about short-circuit evaluation of expression
trees. The future evaluator must preserve error and callback order before it
invokes a primitive. `ArithmeticError` retains expression, condition, phase,
requirement and exact overflow value; source paths, full diagnostic envelopes,
handler observations and counts remain unimplemented.

The checked-in TSV contains explicit expected results, including raw binary64
bits (not digests). Both test runners compare to those expectations; neither
writes them. Temporal tests separately exercise strict lexical forms, leap
centuries, zone rejection, precision retention and loss on text round trips.

No new native API, default backend, benchmark manifest entry, or golden artifact
changes in this slice. Build qualification and the outstanding Cargo-lockfile
policy / R compiled-code warning are recorded in [`README.md`](README.md).
