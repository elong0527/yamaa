---
id: R013
title: Aggregate Reduction
status: normative
applies_to: [expression.aggregate, aggregate_class.between,
  aggregate_expression]

---

# Aggregate reduction

## Intent

Reduce many records to one value with one expression. Avoid one registry entry
per reducer and avoid host-language code.

## Boundaries

This rule owns the `aggregate_expression` primitive: its grammar, reducer
vocabulary, grain rule, result semantics, and failure conditions. R007 owns
the three contexts an aggregate is valid in. R003 owns the join that consumes
a right-side reduction. R004 owns the Boolean `filter`. String reductions use
R019's text equality and total order.

Arithmetic outside a reduction is R010's, reused by reference: its operators,
precedence, function table, numeric types, promotion, and failure conditions
apply here unchanged and are not restated. R010 stays per-row and admits no
reduction; this rule adds reductions and admits no window, `CASE`, comparison,
or Boolean construct.

Ordering and choosing one record from several stay with the window expressions
R007 defines and with `multiple_matches` under R003. `ONLY` does not choose: it
accepts exactly one record and fails when several are present.

## Scope

**R013-1.** An `aggregate_expression` evaluates records from one relation and
returns one value per group. The expression never changes row count. R003 joins
a right-side reduction to constructed rows. An output-row reduction broadcasts
under R007. A grouped row template asks the expression for one value while R001
owns whether that candidate row is appended.

## Relations and identifiers

**R013-2.** An identifier is `NAME` or `DATASET.NAME`. R002 resolves each
identifier in the same phase. A reducer expression and predicate never
disagree about a name.

**R013-3.** Every identifier in one expression must name one relation. Three
forms exist and must not be mixed:

- **Qualified.** Every identifier names the same declared dataset relation.
  During column derivation the expression reduces that right side before the
  R003 join, even when the expression qualifier equals the current row
  template's input dataset. A scalar source qualified to the row
  template's input dataset reads one record; the aggregate keyword makes
  the same qualifier relational.
- **Unqualified.** Every identifier names a current-output column. The
  expression reduces constructed output rows within its `group_by` partition
  and broadcasts the result, which is R007's second aggregate context.
- **Grouped input.** Every identifier is qualified
  to the input dataset of the enclosing grouped row template. The
  expression reduces only the records of the current input group, which
  is R007's third aggregate context.

**R013-4.** A single expression naming two datasets, or mixing a qualified
identifier with an unqualified one, is an error. A reduction is not a join. An
expression combining two dataset relations first binds each relation to a
column and then combines the results with `compute`. R010 admits a qualified
identifier only for a record selected by an R015 record lookup; it
still rejects an arbitrary dataset-qualified identifier, so every join remains
under R003 or R015.

**R013-5.** An ODM contextual reference is not available in this grammar,
because its item identifiers carry further periods. Bind it with a structured
`source` first.

**R013-6.** `group_by` follows the first two forms. An ordinary qualified
expression declares qualified right-side columns, each of which must also be an
output key, so the reduction stays coarser than or equal to the applicable keys
R003 joins on. An unqualified expression declares current-output columns and
must declare at least one: a reduction over the whole output is not registered,
because no example needs one. A grouped-row aggregate declares no local
`group_by`; the enclosing `row.group_by` already fixes its current relation and
grain.

## Row-relative range narrowing

**R013-7.** A qualified aggregate may declare `between` to narrow right-side
records for each current row. `value` is a variable the current row can read.
`lower` and `upper` are qualified columns of the aggregate expression's one
right-side relation, and at least one is required. Every declared comparison is
inclusive: `lower <= value` and `value <= upper`. Omitting one bound makes the
match one-sided without excluding the stated endpoint.

**R013-8.** The value and every stated bound must be mutually comparable under
R007. A missing current-row value admits no right-side record, so the aggregate
result is missing under the empty-group rule below. A right-side record with a
missing stated bound is ineligible. A missing cutoff never causes an
implementation to reduce the unrestricted right side.

**R013-9.** `between` is invalid on an unqualified or grouped-row aggregate:
neither has a separate right-side relation to narrow for each current row.

## Grammar

**R013-10.** The grammar is:

```text
expr       := term (("+" | "-") term)*
term       := factor (("*" | "/") factor)*
factor     := ("-" | "+")? primary
primary    := number | "NULL" | identifier | reduction | call | "(" expr ")"
reduction  := reducer "(" (expr | star) ")"
star       := name "." "*"
call       := function "(" [expr ("," expr)*] ")"
identifier := name ["." name]
number     := digits ["." digits] [("e" | "E") ["+" | "-"] digits]
```

`grammar/aggregate.yaml` is this grammar's only source. The grammar block
above renders its content. Its vocabulary closes the reducer table below.
Its cases record the text each implementation must accept or reject, how
accepted identifiers bind, and the parse each case produces. Repository
validation and the R implementation read that file, so the grammar cannot drift
without failure.

**R013-11.** Precedence, associativity, and the permitted `function` names are
R010's. Reducer and function names and `NULL` are case-insensitive; identifiers
are not.

**R013-12.** Permitted reducers are exactly:

| Reducer | Result |
|---|---|
| `SUM(x)` | total of the non-missing values |
| `COUNT(x)` | how many values of `x` are non-missing |
| `COUNT(D.*)` | how many records the group contains |
| `MIN(x)` | smallest non-missing value |
| `MAX(x)` | largest non-missing value |
| `MEAN(x)` | arithmetic mean of the non-missing numeric values |
| `ONLY(x)` | value from the group's only record; more than one fails |

**R013-13.** Any other reducer name, any window function or `OVER`, any
subquery, any `CASE`, any comparison or Boolean operator, any string literal,
and any host-language call are validation errors. Closing the vocabulary
makes portability checkable; widening it requires amending this table, the
whole cost of a new reduction.

**R013-14.** For a group with at least one non-missing value, `MEAN(x)` is
evaluated as `SUM(x) / COUNT(x)` under this rule's `SUM` semantics and R010's
`/` semantics. The defined division fixes the result and failure behavior
across runtimes without a host language mean.

**R013-15.** `SUM(x)` is a left fold of the non-missing argument values in
relation record order. The accumulator starts with the first such value, and
each later value is added using R010's `+` semantics. Implementations must
not reorder, reassociate, partition, or use a compensated or correctly rounded
summation. The `filter`, when present, removes records without changing the
order of those that remain. R014 defines stored-source record order, and R001
defines constructed-output and grouped-input record order. `MEAN` uses the
same ordered `SUM`, followed by division by `COUNT`, so `MEAN` inherits the
fold's binary64 rounding behavior.

**R013-16.** `AVG` is not an alias; the portable reducer name is `MEAN`. A
median would have to fix its interpolation rule before two runtimes could
agree, so it is not registered by default.

**R013-17.** `ONLY` counts records, not non-missing values. An eligible group
with one record returns that record's value even when missing. An eligible
group with more than one record fails rather than choosing by value or record
order. It is the reduction for a grouped calculation that requires one source
record and must reject duplicates.

**R013-18.** Reductions do not nest. A reduction argument must contain no
reduction, so `MAX(SUM(EX.EXDOSE))` is an error. Reducing at one grain and
reducing that result at another uses two specifications. The first artifact
names and validates the intermediate grain, and the downstream specification
declares that stored artifact as an ordinary source under R002. Pipeline
orchestration supplies the execution and materialization boundary; it is not
inferred from a source path.

**R013-19.** `COUNT(D.*)` takes no other argument; in this rule, `D` is a
placeholder for the relation named by the expression's qualified identifiers
(for example, `COUNT(EX.*)`). It is the one reducer that names no column, and
it counts records where `COUNT(x)` counts values.

## The grain rule

**R013-20.** Every identifier must appear inside a reduction, unless it names a
`group_by` column. `SUM(a) / SUM(b)` is legal. `SUM(a) + b` is an error unless
`b` is grouped on, because a value that varies within a group gives the
expression no single answer, and taking one record's value would depend
on record order. For a grouped-row aggregate, the enclosing `row.group_by`
supplies those grouped columns.

**R013-21.** An identifier that is grouped on is constant within the group and
may be used directly, so `SUM(EX.EXDOSE) / EX.EXPLDOS` is legal exactly when
`EX.EXPLDOS` is declared in `group_by`.

## Types

- **R013-22.** A single reduction retains its result type. `COUNT` returns
  `int`; `MEAN` returns `float`. `SUM` retains its argument's numeric type.
  `MIN`, `MAX`, and `ONLY` retain the type they reduce, whatever that type is.
- **R013-23.** An expression using any operator or R010 function is numeric.
  Every reduction and grouped identifier in it must be numeric, and R010's
  promotion rules give the result type.
- **R013-24.** `SUM` and `MEAN` require a numeric argument. `MIN` and `MAX`
  require mutually comparable values; a column mixing incomparable types is an
  error rather than an implementation-defined order. Their string order is
  R019's. `COUNT` and `ONLY` accept any type.

**R013-25.** R011 converts a completed derivation result, as it does for every
other expression. No implicit conversion happens inside this grammar.

## Missing values and empty groups

**R013-26.** Inside a reduction's argument, `NULL` propagates under R010, so a
record whose operand is missing contributes a missing value rather than a zero.
`SUM(EX.EXDOSE * EX.EXDUR)` skips a record missing either factor.

**R013-27.** A reduction then ignores missing values; the table pins the
rest, because the three target runtimes disagree:

| Condition | Result |
|---|---|
| No record in the group after `filter` | missing, as R003's absent match |
| Every value missing -- `SUM`, `MIN`, `MAX`, `MEAN` | missing, never zero |
| Every value missing -- `COUNT(x)` | `0`, because the records exist |
| No record in the group -- `COUNT(x)`, `COUNT(D.*)` | missing |
| No record in the group -- `ONLY(x)` | missing |
| One record whose value is missing -- `ONLY(x)` | missing |

**R013-28.** An uncollected quantity is never reported as a measured zero, and
an absent record remains distinguishable from a collected missing value.

**R013-29.** More than one record reaching `ONLY` is not a missing-value case.
It fails the current derivation and reports the group values and record count.

**R013-30.** `MEAN` returns missing before its defined division when no
non-missing value remains, so an all-missing group does not fail with division
by zero. Arithmetic over reduction results follows R010: a missing reduction
propagates through an operator, and a formula that must yield missing rather
than fail says so with `NULLIF`.

## Failure conditions

**R013-31.** R010's failure conditions apply to the arithmetic unchanged:
division by zero, `SQRT` of a negative argument, `LN` of a non-positive
argument, invalid `POWER`, and integer overflow each fail the run. R011's non-
finite normalization applies after every arithmetic or reduction result. `SUM`
fails on integer overflow for the same reason. Because `MEAN` is defined by
`SUM`, the same intermediate overflow fails even when the mathematical mean
would fit.

## Determinism

**R013-32.** Evaluation must be deterministic and side-effect free, and R
and Python must produce identical results for every example. R010's determinism
requirements apply unchanged, including that an implementation must not
reassociate or algebraically simplify a written expression.

**R013-33.** A reduction does not sort the records. `SUM` and therefore
`MEAN` consume relation record order as specified above; `COUNT`, `MIN`, and
`MAX` are independent of that order, while `ONLY` accepts no group in which
an order could choose among records. A rule that needs one record chosen by
value order still uses a window or `multiple_matches`, where the value order
is declared.

## Rationale

One expression with a closed reducer vocabulary keeps reductions portable.
Anything outside the table fails validation, and no host dialect applies.
A left-fold `SUM` in relation record order pins binary64 rounding identically
in R and Python. `MEAN` inherits the fold through its defined division.
Missing handling is pinned. Target runtimes disagree, so an uncollected
quantity stays missing and an absent group stays distinguishable from
a collected zero. `ONLY` rejects rather than chooses, so a one-record
calculation cannot silently depend on order. Choosing by value order stays
with windows and `multiple_matches`, which declare the value order.

## Errors

**R013-34.** An `aggregate_expression` that does not parse under this grammar:
fail. **R013-35.** A reducer name outside the table, or one called with a
prohibited argument count: fail. **R013-36.** `ONLY` receiving more than one
record after its filter: fail, reporting the enclosing row, group values, and
record count. **R013-37.** A nested reduction: fail, reporting the outer and
inner reducers. **R013-38.** An identifier outside a reduction that is not a
`group_by` column: fail, reporting the identifier. **R013-39.** An expression
naming more than one dataset, or mixing a qualified identifier with an
unqualified one: fail. **R013-40.** A `COUNT(D.*)` whose dataset is not the
expression's relation: fail. **R013-41.** An ODM contextual reference: fail.
**R013-42.** A qualified `group_by` column that is not an output key, or an
unqualified expression with no `group_by`: fail. **R013-43.** A grouped-row
aggregate declaring its own `group_by`, naming an identifier outside its
row template's input dataset, or being used by an ungrouped row template:
fail. **R013-44.** A
`between` on an unqualified or grouped-row aggregate, declaring neither bound,
naming a bound outside the qualified relation, or using incomparable operands:
fail. **R013-45.** `SUM` or `MEAN` over a non-numeric argument, or arithmetic
over a non-numeric reduction or grouped identifier: fail. **R013-46.** `MIN` or
`MAX` over incomparable values: fail. **R013-47.** A window,
`CASE`, comparison, Boolean, string, subquery, or host construct: fail.
**R013-48.** Any R010 failure condition reached through the arithmetic: fail,
reporting the expression and the column that failed.
