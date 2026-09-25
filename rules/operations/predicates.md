---
id: operations/predicates
title: Predicates
status: normative
---

# Predicates

## Purpose

Evaluate the closed Boolean language using three-valued logic.

## Scope and dependencies

This contract owns the requirements below. Related contracts:

- [Execution lifecycle](../execution/lifecycle.md).
- [Verification](../execution/verification.md).
- [Numeric computation](computation.md).
- [Lookup and joins](lookup.md).
- [Name binding](../specification/binding.md).
- [Temporal values](../values/temporal.md).
- [Text values](../values/text.md).
- [Types and conversion](../values/types.md).


## Requirements

### Predicate sites and results

<a id="req-0158"></a>

**REQ-0158.** The `predicate` primitive is Boolean-valued. It is used by row,
aggregate, window, record-lookup, and multiple-match filters; by `case`;
and by the `assert` and `implies` verifications.

<a id="req-0159"></a>

**REQ-0159.** A predicate evaluates to `TRUE`, `FALSE`, or `UNKNOWN`. A filter
retains a row or record only for `TRUE`. A verification holds only for `TRUE`.
[Verification](../execution/verification.md) defines every other consequence.

### Grammar

<a id="req-0160"></a>

**REQ-0160.** ```text
predicate   := disjunction
disjunction := conjunction ("OR" conjunction)*
conjunction := negation ("AND" negation)*
negation    := "NOT"* boolean
boolean     := comparison | null_test | call | "(" predicate ")" | "TRUE" | "FALSE"
comparison  := operand compare operand
             | operand ["NOT"] "IN" "(" operand ("," operand)* ")"
             | operand ["NOT"] "BETWEEN" operand "AND" operand
             | operand ["NOT"] "LIKE" operand ["ESCAPE" string]
null_test   := operand "IS" ["NOT"] "NULL"
call        := "str_contains" "(" operand "," string ")"
compare     := "=" | "<>" | "<" | "<=" | ">" | ">="
operand     := identifier | literal
identifier  := name ["." name]
name        := (letter | "_") { letter | digit | "_" }
literal     := number | string | temporal | "NULL"
number      := ["+" | "-"] digits ["." digits]
               [("e" | "E") ["+" | "-"] digits]
digits      := digit { digit }
letter      := "A" ... "Z" | "a" ... "z"
digit       := "0" ... "9"
string      := "'" { non_quote | "''" } "'"
non_quote   := any REQ-0022 string scalar other than "'"
temporal    := "DATE" string | "DATETIME" string
```

`grammar/predicate.yaml` is the grammar source. The block renders the file. Its
`reserved` list closes the keywords below. Its cases state the text each
implementation must accept or reject, the identifiers in accepted text, and
each accepted parse. Repository validation and the R implementation read the
file. A copied grammar that differs fails validation.

<a id="req-0161"></a>

**REQ-0161.** Whitespace may separate tokens but cannot occur inside a number,
identifier, or keyword. Precedence is `NOT`, then `AND`, then `OR`. Repeated
binary operators associate from the left. Parentheses override precedence.
Keywords, `NULL`, `TRUE`, and `FALSE` are case-insensitive. Identifiers are
case-sensitive. `AND`, `BETWEEN`, `DATE`, `DATETIME`, `ESCAPE`, `FALSE`, `IN`,
`IS`, `LIKE`, `NOT`, `NULL`, `OR`, and `TRUE` are reserved as bare names. A
qualified field may use one of those spellings after its qualifier.

<a id="req-0162"></a>

**REQ-0162.** An operand is only a name or literal. Arithmetic, `CASE`,
aggregates, windows, subqueries, host-language calls, and `!=` are not in
the grammar. The one function call admitted is the Boolean substring call
REQ-1244 documents. A value computed before comparison is first bound to a
named column. An internal column may be omitted from `output.columns`.

### Literals

<a id="req-0163"></a>

**REQ-0163.** A `number` has [Numeric computation](computation.md)'s number
form and may have a leading sign. It has runtime type `int` when it has neither
a fractional part nor an exponent, and `float` otherwise.

<a id="req-0164"></a>

**REQ-0164.** A `string` is delimited by single quotes. A doubled quote denotes
one quote. Backslash has no escape meaning, so `'C:\new'` contains a
backslash. The literal has runtime type `str` under [Text values](../values/text.md).

<a id="req-0165"></a>

**REQ-0165.** A temporal literal is `DATE '...'` or `DATETIME '...'`. Its text
must parse under [Temporal values](../values/temporal.md) for the named type. The keyword is required:
`'2025-06-01'` alone is a `str`, not a `date`.

<a id="req-0166"></a>

**REQ-0166.** `NULL` is missing and has no runtime type. A comparison with a missing
operand is `UNKNOWN`; `IS NULL` and `IS NOT NULL` are the tests for missingness
and are never `UNKNOWN`.

### Comparison

<a id="req-0167"></a>

**REQ-0167.** No operand is converted. Two non-missing operands compare only
when [REQ-0005](../values/types.md#req-0005) makes their runtime types mutually comparable:

| Types | Order |
|---|---|
| `int` and `float`, in any combination | Numeric after [Numeric values](../values/numbers.md) promotion |
| `str` with `str` | [Text values](../values/text.md) text order |
| `date` with `date` | Chronological under [Temporal values](../values/temporal.md) |
| `datetime` with `datetime` | Chronological under [Temporal values](../values/temporal.md) |

<a id="req-0168"></a>

**REQ-0168.** Every other pair fails. In particular, a temporal value is not
comparable to text, and a `date` is not comparable to a `datetime`.

<a id="req-0169"></a>

**REQ-0169.** Strings use the equality and total order [Text values](../values/text.md) defines.

### Three-valued logic

<a id="req-0170"></a>

**REQ-0170.** Retired. The missing-operand comparison rule is stated once in
[REQ-0166](#req-0166). This identifier is never reused.

<a id="req-0171"></a>

**REQ-0171.** The connectives follow the tables below. `NOT TRUE` is `FALSE`,
`NOT FALSE` is `TRUE`, and `NOT UNKNOWN` is `UNKNOWN`.

| `AND` | TRUE | FALSE | UNKNOWN |
|---|---|---|---|
| **TRUE** | TRUE | FALSE | UNKNOWN |
| **FALSE** | FALSE | FALSE | FALSE |
| **UNKNOWN** | UNKNOWN | FALSE | UNKNOWN |

| `OR` | TRUE | FALSE | UNKNOWN |
|---|---|---|---|
| **TRUE** | TRUE | TRUE | TRUE |
| **FALSE** | TRUE | FALSE | UNKNOWN |
| **UNKNOWN** | TRUE | UNKNOWN | UNKNOWN |

### Compound operators

<a id="req-0172"></a>

**REQ-0172.** Compound operators expand into the primitive logic above:

<a id="req-0173"></a>

**REQ-0173.** `x IN (a, b, c)` is `x = a OR x = b OR x = c`. Every
  non-missing list operand must be comparable with `x`.

<a id="req-0174"></a>

**REQ-0174.** `x BETWEEN a AND b` is `x >= a AND x <= b`; both endpoints are
  inclusive.

<a id="req-0175"></a>

**REQ-0175.** `NOT IN`, `NOT BETWEEN`, and `NOT LIKE` negate the
  corresponding result. A missing operand therefore produces `UNKNOWN`, not
  `TRUE`.

<a id="req-0176"></a>

**REQ-0176.** For `LIKE`, both non-missing operands must be `str`. In the
pattern, `%` matches any sequence of [Text values](../values/text.md) scalar values, `_` matches exactly
one, and every other scalar matches by [Text values](../values/text.md) equality. Matching is
case-sensitive.

<a id="req-0177"></a>

**REQ-0177.** No escape character exists by default. `ESCAPE` declares a string
literal of exactly one [Text values](../values/text.md) scalar value. That character marks the next
pattern scalar as literal; a trailing escape character is invalid.

```yaml
filter: "AEDECOD LIKE '100!%' ESCAPE '!'"
```

### Identifier resolution

<a id="req-0178"></a>

**REQ-0178.** [Execution lifecycle](../execution/lifecycle.md) defines the
names visible at each predicate site:

<a id="req-0179"></a>

**REQ-0179.** an ungrouped row filter sees only fields of its row
  template's input dataset;

<a id="req-0180"></a>

**REQ-0180.** a grouped row filter sees only unqualified columns derived by
  that row;

<a id="req-0181"></a>

**REQ-0181.** aggregate, record-lookup, and multiple-match filters see
  records of their owning right-side dataset;

<a id="req-0182"></a>

**REQ-0182.** a window filter sees completed output columns;

<a id="req-0183"></a>

**REQ-0183.** a verification sees completed columns and record intermediates
  resolved for the completed row; and

<a id="req-0184"></a>

**REQ-0184.** a `case` sees the values available to its enclosing
  derivation.

<a id="req-0185"></a>

**REQ-0185.** An identifier in a right-side predicate is qualified by the
right-side dataset's ID. An identifier over a completed or candidate output
row is unqualified. An enclosing column derivation may also bind a qualified
dataset or record-lookup field under [Name binding](../specification/binding.md), [Lookup and joins](lookup.md), and [Lookup and joins](lookup.md). A verification
may read a declared record lookup for its completed row. No predicate can
reach an undeclared relation.

<a id="req-0186"></a>

**REQ-0186.** [Execution lifecycle](../execution/lifecycle.md) collects predicate identifiers for dependency ordering. A
parser must therefore reject an unresolved name rather than treating a
predicate as dependency-free.

### Determinism

<a id="req-0187"></a>

**REQ-0187.** [Numeric computation](computation.md#req-0436)'s determinism
requirement applies unchanged. A conforming implementation must use
[Text values](../values/text.md) for string comparison.
The implementation must not inherit implicit coercion, collation, `LIKE`
escape, or missing-value behavior from a host SQL engine. The implementation
must either configure and override those behaviors to match these rules or
evaluate the grammar itself.

## Error conditions

<a id="req-0188"></a>

**REQ-0188.** Text that does not parse as one Boolean predicate, including a
  prohibited operator or construct: fail with `invalid_predicate`.

<a id="req-1244"></a>

**REQ-1244.** `str_contains(source, pattern)` is the one function call the
  predicate grammar admits. `source` is any operand; `pattern` is a string
  literal holding a portable regex. The call is `TRUE` when the regex finds
  a match anywhere in the source, `FALSE` when it does not, and `UNKNOWN`
  when the source is missing (including under `NOT`). A non-`str` source
  fails with `incompatible_input_type`; a pattern the portable regex
  contract rejects fails with `invalid_predicate` at parse time. Function-name
  matching is case-insensitive, so `STR_CONTAINS(...)` is the same call, but
  `str_contains` is the sole Boolean function the grammar permits: any other
  `name(...)` is `invalid_predicate`, and a bare `str_contains` without `(`
  stays an identifier.

<a id="req-0189"></a>

**REQ-0189.** An identifier that is unavailable at its predicate site: fail.
  An unqualified identifier that names a field of an in-scope dataset fails
  with `unresolvable_name` and suggests the qualified spelling; any other
  unavailable identifier fails with `unknown_field`. Both report under
  [Execution lifecycle](../execution/lifecycle.md).

<a id="req-0190"></a>

**REQ-0190.** Two non-missing operands that are not mutually comparable, or a
  non-string operand to `LIKE`: fail with `incompatible_input_type`.

<a id="req-0191"></a>

**REQ-0191.** An invalid `ESCAPE` literal or a pattern with a dangling
  escape: fail with `invalid_predicate`.

<a id="req-0192"></a>

**REQ-0192.** A temporal literal that [Temporal values](../values/temporal.md) rejects: fail with [Temporal values](../values/temporal.md)'s
  applicable temporal condition.

## Conformance examples

Representative specifications, input data, and expected outcomes:

- [negative-review-arithmetic](../../benchmarks/negative-review-arithmetic/README.md).
- [negative-review-date-text](../../benchmarks/negative-review-date-text/README.md).
- [negative-review-unknown-date](../../benchmarks/negative-review-unknown-date/README.md).

The [execution manifest](../../benchmarks/execution-manifest.yaml) records
which fixtures execute. Grammar contracts additionally replay their shared
vectors. Static validation does not establish runtime parity.

## Rationale

Evaluate the closed Boolean language using three-valued logic. This topic lets
other owners refer to one policy.
