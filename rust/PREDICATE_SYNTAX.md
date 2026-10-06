# Shared predicate syntax

`yamaa_core::predicate_parser::parse_predicate` parses the closed R004 grammar
into an immutable postorder arena. `yamaa_native.analyze_predicate` and
`yamaanative::analyze_predicate` expose the same `predicate-syntax/1` service.
This service admits syntax only. It does not resolve names, evaluate predicates,
read data, activate callbacks or enable dataset execution. Python remains the
default and `execution_supported=false`.

## Request and outcomes

A request has exactly two string fields:

```json
{"protocol":"predicate-syntax/1","expression":"NOT str_contains(A, 'X') OR B IS NULL"}
```

Missing, unknown and duplicate fields, wrong JSON types, malformed JSON and
invalid Unicode escapes are transport errors. The full request is decoded before
the expression is parsed. The 1,048,576-byte envelope limit precedes decoding.
Ordinary transport errors become Python `ValueError` or an R error after the
native call returns. Panics become fixed internal failures without exposing
payloads or partial ASTs.

The response contains `protocol` and `outcome`. A `parsed` outcome contains the
portable `ast` and unique `identifiers` in first written order. Number literals
remain their original text, including signs, zeros and exponent spelling. No
numeric conversion or overflow decision is made here. Strings decode doubled
single quotes once; backslashes, NUL and Unicode scalars remain literal. Temporal
text is checked by the shared civil parser and retained in its original spelling.
Grouping is retained in the core arena and elided in the JSON AST without changing
association. Operand and call positions count original Unicode scalar offsets.

An `invalid` outcome carries `invalid_predicate`, its owning requirement, a
zero-based `position` with both UTF-8 `byte` and Unicode scalar `character`
offsets, and typed `context`. Invalid portable regex literals belong to
REQ-1244. Their outer position is the opening quote in the predicate, and
`context.pattern_byte` is the byte offset in the decoded regex literal. No
incorrect offset into the original doubled-quote spelling is fabricated.
`context.reason` is the regex compiler's fixed rejection reason.

Whole-input tokenization precedes parsing, matching existing lexical failure
order. Every regex literal is compiled during parsing, before requiring the
call's closing parenthesis and without inspecting a subject. The case-insensitive
`str_contains` call is recognized only with `(`; its bare spelling remains an
ordinary identifier. All names, including qualified reserved spellings such as
`D.NULL`, retain their case. No Python/R regex or predicate parser is used.

A `resource_limit` outcome has a `phase` (`parse` or `regex_compile`), resource,
limit and original source position. Reserved `unsupported` regex outcomes remain
separate from invalid grammar and resource exhaustion. Failed calls do not retain
state, and a new call can succeed.

## Policies and diagnostic differences

Default parse policies are 65,536 expression bytes, 8,192 tokens excluding EOF,
4,096 nodes including groups/operands, and depth 64. Depth also bounds recursive
parsing and output construction, including repeated NOT and flat Boolean chains.
The flat arena has no recursive ownership chain. Regex compile policies apply
to each pattern. In addition, all literals in one predicate share capture-dependent
width-analysis ceilings of 1,000,000 work units and 1,000,000 logical storage cells.
Charges precede analysis/allocation and failed compilations retain consumed work;
the resource outcome points to the literal that exhausted the request quota.
Byte/token/node limits bound total pattern source and parser structure. These
quotas are request-local, not shared across requests or concurrent callers.
AST construction and serialization are
bounded by the admitted tree and source size; there is no independent streaming
response-byte policy. Host allocation before entry, allocator capacity/failure,
process-wide memory and cancellation are outside these logical policies.

Two diagnostic differences must remain visible:

- Invalid static ESCAPE values/dangling escapes report REQ-0191. Python currently
  attributes these parser errors to REQ-0188. The condition and source position
  agree. Tests name the three compared cases explicitly.
- The committed grammar rejects an invalid temporal literal with
  `invalid_predicate`, as does Python. This service preserves that grammar result
  and REQ-0188, while retaining `literal_type` and `temporal_error` in context.
  REQ-0192 instead refers to applicable temporal conditions. That specification
  inconsistency is unresolved; this slice does not claim temporal diagnostic
  qualification or rewrite the committed expected outcome.

## Evidence and remaining work

Thirty-two explicitly authored wire cases are replayed through Rust and both
installed hosts. Core tests exercise precedence, all comparisons, reserved names,
number spelling, exact strings, temporal rejection, regex validation order,
Unicode coordinates, resource refusal and fresh retry. The grammar checker reads
the unchanged committed contract and adds named call/ESCAPE cases: 72 independent
checks and 1,083 supplemental reference observations. Reference comparisons are
additional evidence, not the expected truth. Known portable regex acceptance
cases rejected by Python are authored independently and do not use Python as the
admission oracle.

A regression repeats a legal capture-dependent pattern 512 times in a balanced
42,490-byte expression that fits the parser limits. It exhausted no per-pattern
quota before the fix; it now refuses at the cumulative width-work ceiling. Rust
and both installed hosts check this outcome and fresh-request recovery. Core
tests also check cumulative storage limits, independent per-pattern ceilings,
consumed prefixes after grammar failure, overflow and the static fast path.

The Python specification frontend still uses its existing predicate parser.
Replacing every planning/lowering path with this service and adding native
`str_contains` evaluation are the next integration gate. Regex matching must
charge cumulative dataset budgets across rows and verification declarations;
per-call limits alone are insufficient there. The typed predicate evaluator,
full shared specification compiler, all consumer semantics, workflow/R APIs,
release qualification and separate POWER/EXP/LN policy remain required.
