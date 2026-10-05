# Shared numeric syntax

`numeric-syntax/1` exposes the existing bounded R010 Rust parser through
`analyze_numeric(request)` in Python and R. It returns syntax and ordered reads
without resolving names, evaluating literals or granting execution capability.
Python stays default and `execution_supported` stays false.

A request contains exactly `protocol` and `expression`:

```json
{"protocol":"numeric-syntax/1","expression":"COALESCE(D.X, 0) * 2"}
```

A `parsed` outcome contains an `ast` and ordered distinct `identifiers`. AST
nodes are number, null, identifier, unary, binary and call. Exact number text
and int/float syntax tags survive unchanged, including values whose eventual
evaluation overflows. Groups control association without adding wire nodes.
Function names are canonical; identifiers retain case and first occurrence.
All declared numeric functions parse, including POWER/EXP/LN, but aggregate
reducers and record stars do not. Function acceptance is independent of the
separately qualified evaluator policy.

An `invalid` outcome contains the owning condition, requirement and context,
plus zero-based UTF-8 byte and Unicode scalar offsets. Lexing precedes syntax
validation and completed calls precede name/arity checks. The wire service
preserves these existing parser priorities. Resource exhaustion produces a
separate `resource_limit` outcome naming the resource, limit and position.
Budgets remain 65,536 source bytes, 8,192 tokens excluding EOF, 4,096 arena nodes
including groups, and depth 64. Requests cannot raise them. Malformed JSON,
duplicate/unknown fields, wrong types/protocol and requests over 1,048,576 bytes
are transport errors. Native R returns normally before its facade raises an
error; failed requests cannot poison a later independent parse.

The temporary Python planner accepts an explicit numeric-analysis port. The
default port retains the reference parser. The optional native frontend captures
the Rust service before activation/data, then shares its AST and metadata through
admission, dependency discovery (including aggregate derive steps), inferred-key
planning and lowering. Each run has a bounded 512-entry cache. Consumers treat
the AST as read-only; no Python metadata walk or parser fallback occurs. Native
transport/resource errors remain distinct from language diagnostics. Authored
diagnostics retain their existing expression path and context.

Sixteen independently authored TSV observations are replayed by Rust and both
installed hosts. `check_numeric_syntax.py` replays every committed grammar case
and vocabulary expansion, then compares complete ASTs, diagnostics and ordered
metadata against unchanged Python. Installed integration tests forbid reference
syntax, check exact authored CSV, service capture across provider mutation,
grammar/resource failures before host effects and unqualified numerical-policy
refusal. Both native wheel forms and the standalone R package run these checks.
Native CI includes R facade-only changes in its path filters so those changes
cannot skip installed adapter qualification.

Context/type rules, predicate syntax, normalization/inheritance, complete shared
compilation and R current-schema execution remain separate work. This service
does not qualify new numerical functions, additional execution families, full
cross-language compatibility or a default-backend cutover.
