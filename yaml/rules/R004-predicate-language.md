---
id: R004
title: Predicate Language
status: normative
applies_to: [sql]

---

# Predicate language

## Intent

Define the portable Boolean predicate in a `sql` field: grammar,
literals, comparisons, missing-value behavior, and failures.

## Boundaries

This rule owns the `sql` primitive completely. R006 owns schema
structure. R007 owns runtime types and comparability of operation
inputs. R010 owns the numeric-valued `numeric_expression` primitive.
R011 owns the column type vocabulary. R016 owns temporal values. R019
owns text values and their equality and order. R001 owns the phase in
which a predicate runs and the names available in that phase.

The predicate and numeric primitives share identifier notation and numeric
literals. Neither grammar admits the other's operators or functions.

## Predicate sites and results

**R004-1.** The `sql` primitive is Boolean-valued. It is used by row,
aggregate, window, record-lookup, and multiple-match filters; by `case`;
and by the `predicate` and `implies` verifications.

**R004-2.** A predicate evaluates to `TRUE`, `FALSE`, or `UNKNOWN`. A filter
retains a row or record only for `TRUE`. A verification holds only for
`TRUE`; otherwise R009 defines the consequence.

## Grammar

```text
predicate   := disjunction
disjunction := conjunction ("OR" conjunction)*
conjunction := negation ("AND" negation)*
negation    := "NOT"* boolean
boolean     := comparison | null_test | "(" predicate ")" | "TRUE" | "FALSE"
comparison  := operand compare operand
             | operand ["NOT"] "IN" "(" operand ("," operand)* ")"
             | operand ["NOT"] "BETWEEN" operand "AND" operand
             | operand ["NOT"] "LIKE" operand ["ESCAPE" string]
null_test   := operand "IS" ["NOT"] "NULL"
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
non_quote   := any R019 string scalar other than "'"
temporal    := "DATE" string | "DATETIME" string
```

**R004-3.** `grammar/predicate.yaml` is the single grammar source. The block
renders the file. Its `reserved` list closes the keywords below. Its cases
state which text each implementation must accept or reject, which identifiers
accepted text binds, and each accepted parse. Repository validation
and the R implementation read the file. A copied grammar that differs from the
file fails validation.

**R004-4.** Whitespace may separate tokens but cannot occur inside a number,
identifier, or keyword. Precedence is `NOT`, then `AND`, then `OR`. Repeated
binary operators associate from the left. Parentheses override precedence.
Keywords, `NULL`, `TRUE`, and `FALSE` are case-insensitive. Identifiers are
case-sensitive. `AND`, `BETWEEN`, `DATE`, `DATETIME`, `ESCAPE`, `FALSE`, `IN`,
`IS`, `LIKE`, `NOT`, `NULL`, `OR`, and `TRUE` are reserved as bare names. A
qualified field may use one of those spellings after its qualifier.

**R004-5.** An operand is only a name or literal. Arithmetic, function calls,
`CASE`, aggregates, windows, subqueries, host-language calls, and `!=` are not
in the grammar. A value computed before comparison is first bound to a named
column. An internal column may be omitted from `output.columns`.

## Literals

**R004-6.** A `number` is R010's number form with an optional leading sign. It
has runtime type `int` when it has neither a fractional part nor an exponent,
and `float` otherwise.

**R004-7.** A `string` is delimited by single quotes. A doubled quote denotes
one quote. Backslash has no escape meaning, so `'C:\new'` contains a
backslash. The literal has runtime type `str` under R019.

**R004-8.** A temporal literal is `DATE '...'` or `DATETIME '...'`. Its text
must parse under R016 for the named type. The keyword is required:
`'2025-06-01'` alone is a `str`, not a `date`.

**R004-9.** `NULL` is missing and has no runtime type. A comparison with it is
`UNKNOWN`; `IS NULL` and `IS NOT NULL` are the tests for missingness.

## Comparison

**R004-10.** No operand is converted. Two non-missing operands compare only
when R007 makes their runtime types mutually comparable:

| Types | Order |
|---|---|
| `int` and `float`, in any combination | Numeric after R010 promotion |
| `str` with `str` | R019 text order |
| `date` with `date` | Chronological under R016 |
| `datetime` with `datetime` | Chronological under R016 |

**R004-11.** Every other pair fails. In particular, a temporal value is not
comparable to text, and a `date` is not comparable to a `datetime`.

**R004-12.** Strings use the equality and total order R019 defines.

## Three-valued logic

**R004-13.** A comparison with a missing operand is `UNKNOWN`. `IS NULL` and
`IS NOT NULL` are never `UNKNOWN`.

**R004-14.** The connectives follow the tables below. `NOT TRUE` is `FALSE`,
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

## Compound operators

**R004-15.** Compound operators expand into the primitive logic above:

- **R004-16.** `x IN (a, b, c)` is `x = a OR x = b OR x = c`. Every
  non-missing list operand must be comparable with `x`.
- **R004-17.** `x BETWEEN a AND b` is `x >= a AND x <= b`; both endpoints are
  inclusive.
- **R004-18.** `NOT IN`, `NOT BETWEEN`, and `NOT LIKE` negate the
  corresponding result. A missing operand therefore produces `UNKNOWN`, not
  `TRUE`.

**R004-19.** For `LIKE`, both non-missing operands must be `str`. In the
pattern, `%` matches any sequence of R019 scalar values, `_` matches exactly
one, and every other scalar matches by R019 equality. Matching is
case-sensitive.

**R004-20.** No escape character exists by default. `ESCAPE` declares a string
literal of exactly one R019 scalar value. That character marks the next
pattern scalar as literal; a trailing escape character is invalid.

```yaml
filter: "AEDECOD LIKE '100!%' ESCAPE '!'"
```

## Identifier resolution

**R004-21.** R001 defines the names visible at each predicate site. In
summary:

- **R004-22.** an ungrouped row filter sees only fields of its row
  template's input dataset;
- **R004-23.** a grouped row filter sees only unqualified columns derived by
  that row;
- **R004-24.** aggregate, record-lookup, and multiple-match filters see
  records of their owning right-side dataset;
- **R004-25.** a window filter sees completed output columns;
- **R004-26.** a verification sees completed output columns and record intermediates
  resolved for the completed row; and
- **R004-27.** a `case` sees the values available to its enclosing
  derivation.

**R004-28.** An identifier in a right-side predicate is qualified by the
right-side dataset's ID. An identifier over a completed or candidate output
row is unqualified. An enclosing column derivation may also bind a qualified
dataset or record-lookup field under R002, R003, and R015. A verification
may read a declared record lookup for its completed row. No predicate can
reach an undeclared relation.

**R004-29.** R001 collects predicate identifiers for dependency ordering. A
parser must therefore reject an unresolved name rather than treating a
predicate as dependency-free.

## Determinism

**R004-30.** Evaluation is deterministic and side-effect free. A conforming
implementation must use R019 for string comparison. The implementation must
not inherit implicit coercion, collation, `LIKE` escape, or missing-value
behavior from a host SQL engine. The implementation must either configure and
override those behaviors to match these rules or evaluate the grammar itself.

## Rationale

A closed predicate grammar keeps row selection, corrections, and verifications
reviewable and identical in R and Python. The grammar requires a named column
for each value computed before comparison. The named column fixes type and
missing-value behavior. Three-valued logic without implicit conversion
avoids dependence on host SQL coercion, collation, or escape defaults.

## Errors

- **R004-31.** Text that does not parse as one Boolean predicate, including a
  prohibited operator or construct: fail with `invalid_predicate`.
- **R004-32.** An identifier that is unavailable at its predicate site: fail
  with `unknown_field` under R001.
- **R004-33.** Two non-missing operands that are not mutually comparable, or a
  non-string operand to `LIKE`: fail with `incompatible_input_type`.
- **R004-34.** An invalid `ESCAPE` literal or a pattern with a dangling
  escape: fail with `invalid_predicate`.
- **R004-35.** A temporal literal that R016 rejects: fail with R016's
  applicable temporal condition.
