---
id: R013
title: Aggregate Reduction
status: normative
applies_to: [expression.aggregate, aggregate_class.between,
  aggregate_expression]
depends_on: [R001, R002, R003, R004, R006, R007, R010, R011, R015, R019]
---

# Aggregate reduction

## Intent

Reduce many records to one value with one expression, without a registry entry
per reducer and without host-language code.

## Boundaries

This rule owns the `aggregate_expression` primitive: its grammar, reducer
vocabulary, grain rule, result semantics, and failure conditions. It does not
own the three contexts an aggregate is valid in, which is R007, the join that
consumes a right-side reduction, which is R003, or the Boolean `filter`, which
is R004. String reductions use R019's text equality and total order.

Arithmetic outside a reduction is R010's, reused by reference: its operators,
precedence, function table, numeric types, promotion, and failure conditions
apply here unchanged and are not restated. R010 stays per-row and admits no
reduction; this rule adds reduction and admits no window, `CASE`, comparison,
or Boolean construct.

Ordering, and choosing one record from several, stay with the window
expressions R007 defines and with `multiple_matches` under R003. `ONLY` does
not choose: it accepts exactly one record and fails when several are present.

## Scope

An `aggregate_expression` evaluates over the records of one relation and
returns one value per group. Its result is a single value for the group, so the
expression itself never changes row count: R003 joins a right-side reduction
to constructed rows, an output-row reduction broadcasts under R007, and a
grouped row template asks the expression for one value while R001 owns whether
that candidate row is appended.

## Relations and identifiers

An identifier is `NAME` or `DATASET.NAME`, resolved as R002 resolves the same
name in the same phase, so a reducer expression and a predicate never disagree
about a name.

**Every identifier in one expression must name one relation.** Three forms
exist and must not be mixed:

- **Qualified.** Every identifier names the same declared dataset relation.
  During column derivation the expression reduces that right side before the
  R003 join, even when its qualifier equals the current row driver. A scalar
  source qualified to the driver reads one record; the aggregate keyword makes
  the same qualifier relational.
- **Unqualified.** Every identifier names a current-output column. The
  expression reduces constructed output rows within the partition its
  `group_by` declares and broadcasts the result, which is R007's second
  aggregate context.
- **Grouped row driver.** Every identifier is qualified to the row driver of
  the enclosing grouped row template. The expression reduces only the records
  of the current driver group, which is R007's third aggregate context.

A single expression naming two datasets, or mixing a qualified identifier with
an unqualified one, is an error. A reduction is not a join: an expression
combining values from two dataset relations binds each of them to a column
first and composes the results with `compute`. R010 admits a qualified
identifier only for a record already selected by an R015 record lookup; it
still rejects an arbitrary dataset-qualified identifier, so every join remains
under R003 or R015.

An ODM contextual reference is not available in this grammar, because its item
identifiers carry further periods. Bind it with a structured `source` first.

`group_by` follows the first two forms. An ordinary qualified expression
declares qualified right-side columns, each of which must also be an output
key, so the reduction stays coarser than or equal to the applicable keys R003
joins on. An unqualified expression declares current-output columns and must
declare at least one: a reduction over the whole output is not registered,
because no example needs one. A grouped-row aggregate declares no local
`group_by`; the enclosing `row.group_by` already fixes its current relation and
grain.

## Row-relative range narrowing

A qualified aggregate may declare `between` to narrow its right-side records
for each current row. `value` is a variable the current row can read. `lower`
and `upper` are qualified columns of the aggregate expression's one right-side
relation, and at least one is required. Every declared comparison is inclusive:
`lower <= value` and `value <= upper`. Omitting one bound makes the match
one-sided; it does not exclude the stated endpoint.

The value and every stated bound must be mutually comparable under R007. A
missing current-row value admits no right-side record, so the aggregate result
is missing under the empty-group rule below. A right-side record with a missing
stated bound is ineligible. In particular, a missing cutoff never causes an
implementation to reduce the unrestricted right side.

`between` is invalid on an unqualified or grouped-row aggregate: neither has a
separate right-side relation to narrow for each current row.

## Grammar

```text
expr      := term (("+" | "-") term)*
term      := factor (("*" | "/") factor)*
factor    := ("-" | "+")? primary
primary   := number | "NULL" | identifier | reduction | call | "(" expr ")"
reduction := reducer "(" (expr | star) ")"
star      := name "." "*"
call      := function "(" [expr ("," expr)*] ")"
identifier := name ["." name]
number    := digits ["." digits] [("e" | "E") ["+" | "-"] digits]
```

Precedence, associativity, and the permitted `function` names are R010's.
Reducer and function names and `NULL` are case-insensitive; identifiers are
not.

Permitted reducers are exactly:

| Reducer | Result |
|---|---|
| `SUM(x)` | total of the non-missing values |
| `COUNT(x)` | how many values of `x` are non-missing |
| `COUNT(D.*)` | how many records the group contains |
| `MIN(x)` | smallest non-missing value |
| `MAX(x)` | largest non-missing value |
| `MEAN(x)` | arithmetic mean of the non-missing numeric values |
| `ONLY(x)` | the value from the group's only record; more than one record fails |

Any other reducer name, any window function or `OVER`, any subquery, any
`CASE`, any comparison or Boolean operator, any string literal, and any
host-language call are validation errors. Closing the vocabulary is what makes
portability checkable; widening it requires amending this table, and that
amendment is the whole cost of a new reduction.

For a group with at least one non-missing value, `MEAN(x)` is evaluated as
`SUM(x) / COUNT(x)` under this rule's `SUM` semantics and R010's `/` semantics.
This fixes its result and failure behavior across runtimes instead of inheriting
a host language's mean implementation.

`AVG` is not an alias; the portable reducer name is `MEAN`. A median would
additionally have to fix its interpolation rule before two runtimes could
agree, so it is not registered by default.

`ONLY` counts records, not non-missing values. An eligible group with one
record returns that record's value even when it is missing. An eligible group
with more than one record fails rather than choosing by value or record order.
It is the reduction for a grouped calculation that requires one source record
and must reject duplicates.

**Reductions do not nest.** The argument of a reduction must contain no
reduction, so `MAX(SUM(EX.EXDOSE))` is an error. Reducing at one grain and
reducing that result at another uses two specifications: the first artifact
names and validates the intermediate grain, and the downstream specification
declares that stored artifact as an ordinary source under R002. Pipeline
orchestration supplies the execution and materialization boundary; it is not
inferred from a source path.

`COUNT(D.*)` takes no other argument; in this rule, `D` is a placeholder for the dataset named by the expression's qualified identifiers (for example, `COUNT(EX.*)`). It is the one reducer that names no column, and it counts records where `COUNT(x)` counts values.

## The grain rule

**Every identifier must appear inside a reduction, unless it names a
`group_by` column.** `SUM(a) / SUM(b)` is legal. `SUM(a) + b` is an error
unless `b` is grouped on, because a value that varies within the group gives
the expression no single answer, and silently taking one record's value would
depend on record order. For a grouped-row aggregate, the enclosing
`row.group_by` supplies those grouped columns.

An identifier that is grouped on is constant within the group and may be used
directly, so `SUM(EX.EXDOSE) / EX.EXPLDOS` is legal exactly when `EX.EXPLDOS`
is declared in `group_by`.

## Types

- An expression that is a single reduction retains that reduction's result
  type. `COUNT` returns `int`; `MEAN` returns `float`. `SUM` retains the numeric
  type of its argument. `MIN`, `MAX`, and `ONLY` retain the type they reduce,
  whatever that type is.
- An expression using any operator or R010 function is numeric. Every
  reduction and every grouped identifier in it must be numeric, and R010's
  promotion rules give the result type.
- `SUM` and `MEAN` require a numeric argument. `MIN` and `MAX` require mutually
  comparable values; a column mixing incomparable types is an error rather
  than an implementation-defined order. Their string order is R019's.
  `COUNT` and `ONLY` accept any type.

R011 converts the completed derivation result, as it does for every other
expression. No implicit conversion happens inside this grammar.

## Missing values and empty groups

Inside a reduction's argument, `NULL` propagates under R010, so a record whose
operand is missing contributes a missing value rather than a zero:
`SUM(EX.EXDOSE * EX.EXDUR)` skips a record missing either factor.

A reduction then ignores missing values. What remains is pinned here, because
the three runtimes this design targets disagree:

| Condition | Result |
|---|---|
| No record in the group after `filter` | missing, as R003's absent match |
| Every value missing -- `SUM`, `MIN`, `MAX`, `MEAN` | missing, never zero |
| Every value missing -- `COUNT(x)` | `0`, because the records exist |
| No record in the group -- `COUNT(x)`, `COUNT(D.*)` | missing |
| No record in the group -- `ONLY(x)` | missing |
| One record whose value is missing -- `ONLY(x)` | missing |

An uncollected quantity is therefore never reported as a measured zero, and an
absent record stays distinguishable from a collected missing value.

More than one record reaching `ONLY` is not a missing-value case. It fails the
current derivation and reports the group values and record count.

`MEAN` answers missing before applying its defined division when no non-missing
value remains, so an all-missing group does not fail with division by zero.
Arithmetic over reduction results follows R010: a missing reduction propagates
through an operator, and a formula that must yield missing rather than fail
says so with `NULLIF`.

## Failure conditions

R010's failure conditions apply to the arithmetic unchanged: division by zero,
`SQRT` of a negative argument, `LN` of a non-positive argument, invalid
`POWER`, and integer overflow each fail the run. R011's non-finite
normalization applies after every arithmetic or reduction result. `SUM` fails
on integer overflow for the same reason. Because `MEAN` is defined through
`SUM`, the same intermediate overflow fails even when the mathematical mean
would fit.

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
- A reducer name outside the table, or one called with a prohibited argument
  count: fail.
- `ONLY` receiving more than one record after its filter: fail, reporting the
  enclosing row, group values, and record count.
- A nested reduction: fail, reporting the outer and inner reducers.
- An identifier outside every reduction that is not a `group_by` column: fail,
  reporting the identifier.
- An expression naming more than one dataset, or mixing a qualified identifier
  with an unqualified one: fail.
- A `COUNT(D.*)` whose dataset is not the expression's relation: fail.
- An ODM contextual reference: fail.
- A qualified `group_by` column that is not an output key, or an unqualified
  expression with no `group_by`: fail.
- A grouped-row aggregate declaring its own `group_by`, naming an identifier
  outside its row driver, or being used by an ungrouped row template: fail.
- A `between` on an unqualified or grouped-row aggregate, declaring neither
  bound, naming a bound outside the qualified relation, or using incomparable
  operands: fail.
- `SUM` or `MEAN` over a non-numeric argument, or arithmetic over a non-numeric
  reduction or grouped identifier: fail.
- `MIN` or `MAX` over values that are not mutually comparable: fail.
- A window, `CASE`, comparison, Boolean, string, subquery, or host-language
  construct: fail.
- Any R010 failure condition reached through the arithmetic: fail, reporting
  the expression and the column that failed.
