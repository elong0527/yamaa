---
id: R013
title: Aggregate Reduction
status: normative
applies_to: [expression.aggregate, aggregate_expression]
depends_on: [R001, R002, R003, R004, R006, R007, R010, R011, R017]
---

# Aggregate reduction

## Intent

Reduce many records to one value with one expression, without a registry entry
per reducer and without host-language code.

## Boundaries

This rule owns the `aggregate_expression` primitive: its grammar, grain rule,
result semantics, and failure conditions. R017 owns the portable reducer
vocabulary and its machine-readable call contracts. This rule does not own the
two contexts an aggregate is valid in, which is R007, the join that
consumes a right-side reduction, which is R003, or the Boolean `filter`, which
is R004.

Arithmetic outside a reduction is R010's, reused by reference: its operators,
precedence, scalar calls, numeric types, promotion, and failure conditions
apply here unchanged and are not restated. R010 stays per-row and admits no
reduction; this rule adds reduction and admits no window, `CASE`, comparison,
or Boolean construct.

Ordering, and selecting one record rather than reducing many, stay with the
window expressions R007 defines and with `multiple_matches` under R003.

## Scope

An `aggregate_expression` evaluates over the records of one relation and
returns one value per group. Its result is a single value for the group, so a
reduction never changes row count: R003 joins a right-side reduction to the
constructed rows, and an output-row reduction broadcasts under R007.

## Relations and identifiers

An identifier is `NAME` or `DATASET.NAME`, resolved as R002 resolves the same
name in the same phase, so a reducer expression and a predicate never disagree
about a name.

**Every identifier in one expression must name one relation.** Two forms exist
and must not be mixed:

- **Qualified.** Every identifier names the same declared dataset. The
  expression reduces that right side before the R003 join.
- **Unqualified.** Every identifier names a current-output column. The
  expression reduces constructed output rows within the partition its
  `group_by` declares and broadcasts the result, which is R007's second
  aggregate context.

A single expression naming two datasets, or mixing a qualified identifier with
an unqualified one, is an error. A reduction is not a join: an expression
combining values from two relations binds each of them to a column first and
composes the results with `compute`, which keeps R010's ban on qualified
identifiers intact and keeps every join under R003.

An ODM contextual reference is not available in this grammar, because its item
identifiers carry further periods. Bind it with a structured `source` first.

`group_by` follows the same division. A qualified expression declares
qualified right-side columns, each of which must also be an output key, so the
reduction stays coarser than or equal to the applicable keys R003 joins on. An
unqualified expression declares current-output columns and must declare at
least one: a reduction over the whole output is not registered, because no
example needs one.

## Grammar

```text
expr      := term (("+" | "-") term)*
term      := factor (("*" | "/") factor)*
factor    := ("-" | "+")? primary
primary   := number | "NULL" | identifier | reduction | call | "(" expr ")"
reduction := portable_name "(" (expr | star) ")"
star      := name "." "*"
call      := portable_name "(" [expr ("," expr)*] ")"
portable_name := name | namespace "::" name
identifier := name ["." name]
number    := digits ["." digits] [("e" | "E") ["+" | "-"] digits]
```

Precedence, associativity, and scalar calls are R010's. Reducer and scalar
names and `NULL` are case-insensitive; namespaces and identifiers are not.

A reduction resolves through R017 and requires `evaluation_kind: reducer`.
Its signature, input types, result type, missing behavior, failures,
determinism, accuracy, and availability come from the same registry as scalar
calls. The generated inventory is `registry/README.md`. Any other reducer name,
window function or `OVER`, subquery, `CASE`, comparison or Boolean operator,
string literal, or host-language call is a validation error.

`AVG` is not an alias; the portable reducer name is `MEAN`. A median would
additionally have to fix its interpolation rule before two runtimes could
agree, so it is not registered by default.

**Reductions do not nest.** The argument of a reduction must contain no
reduction, so `MAX(SUM(EX.EXDOSE))` is an error. Reducing at one grain and
reducing that result at another needs an intermediate grain the language cannot
name. Naming one is open work.

A qualified star is accepted only when the resolved reducer parameter includes
`record_star`. `D` must be the dataset named by the expression's qualified
identifiers. It names records rather than one column.

## The grain rule

**Every identifier must appear inside a reduction, unless it names a
`group_by` column.** `SUM(a) / SUM(b)` is legal. `SUM(a) + b` is an error
unless `b` is grouped on, because a value that varies within the group gives
the expression no single answer, and silently taking one record's value would
depend on record order.

An identifier that is grouped on is constant within the group and may be used
directly, so `SUM(EX.EXDOSE) / EX.EXPLDOS` is legal exactly when `EX.EXPLDOS`
is declared in `group_by`.

## Types

- An expression that is a single reduction has the `result_type` and
  `type_promotion` declared by its registry entry.
- An expression using any operator or R010 function is numeric. Every
  reduction and every grouped identifier in it must be numeric, and R010's
  promotion rules give the result type.
- A reducer argument must have one of the types its registry parameter accepts.
  A reducer over comparable values additionally requires the R007
  comparability rule; a column mixing incomparable types is an error rather
  than an implementation-defined order.

R011 converts the completed derivation result, as it does for every other
expression. No implicit conversion happens inside this grammar.

## Missing values and empty groups

Inside a reduction's argument, `NULL` propagates under R010, so a record whose
operand is missing contributes a missing value rather than a zero:
`SUM(EX.EXDOSE * EX.EXDUR)` skips a record missing either factor.

A reduction applies its registry entry's `missing_values` contract. No record
in the group after `filter` is still missing as R003's absent match; a reducer
is not evaluated for a group that does not exist.

An uncollected quantity is therefore never reported as a measured zero, and an
absent record stays distinguishable from a collected missing value.

Arithmetic over reduction results follows R010: a missing reduction propagates
through an operator, and a formula that must yield missing rather than fail
says so with `NULLIF`.

## Failure conditions

R010's failure conditions apply to arithmetic unchanged. A reducer applies its
registry entry's domain, overflow, and non-finite result behavior, including
failures reached in an intermediate result named by its declarative definition.

## Determinism

Evaluation must be deterministic and free of side effects, and R and Python
must produce identical results for every example. R010's determinism
requirements apply unchanged, including that an implementation must not
reassociate or algebraically simplify a written expression.

A reduction imposes no order on the records it reads, so its result must not
depend on record order. This is what keeps ordering out of this grammar:
a rule that needs a record chosen by order uses a window or
`multiple_matches`, where the order is declared.

## Errors

- An `aggregate_expression` that does not parse under this grammar: fail.
- A reducer call that fails R017 name, kind, version, arity, or type
  validation: fail with R017's stable condition.
- A nested reduction: fail, reporting the outer and inner reducers.
- An identifier outside every reduction that is not a `group_by` column: fail,
  reporting the identifier.
- An expression naming more than one dataset, or mixing a qualified identifier
  with an unqualified one: fail.
- A qualified star whose reducer does not accept `record_star`, or whose
  dataset is not the expression's relation: fail.
- An ODM contextual reference: fail.
- A qualified `group_by` column that is not an output key, or an unqualified
  expression with no `group_by`: fail.
- Arithmetic over a non-numeric reduction or grouped identifier: fail.
- A reducer over values that are not mutually comparable when its accepted
  input type requires comparison: fail.
- A window, `CASE`, comparison, Boolean, string, subquery, or host-language
  construct: fail.
- Any R010 failure condition reached through the arithmetic: fail, reporting
  the expression and the column that failed.
