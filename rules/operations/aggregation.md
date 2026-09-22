---
id: operations/aggregation
title: Aggregation
status: normative
---

# Aggregation

## Purpose

Reduce eligible records within one of three permitted key scopes.

## Scope and dependencies

This contract owns the requirements below. Related contracts:

- [Execution lifecycle](../execution/lifecycle.md).
- [Numeric computation](computation.md).
- [Project functions](functions.md).
- [Lookup and joins](lookup.md).
- [Predicates](predicates.md).
- [Name binding](../specification/binding.md).
- [Source ingestion](../storage/ingestion.md).
- [Text values](../values/text.md).
- [Types and conversion](../values/types.md).


## Requirements

### Evaluation kinds

<a id="req-0295"></a>

**REQ-0295.** `aggregate` is the only aggregate expression. Its three permitted
evaluation contexts are defined by [REQ-0467](aggregation.md#req-0467); [Aggregation](aggregation.md) owns its grammar, reducers,
and result semantics.

### Type behavior

<a id="req-0313"></a>

**REQ-0313.** `aggregate` states its own input types in [Aggregation](aggregation.md).

<a id="req-0317"></a>

**REQ-0317.** `aggregate` returns the type [Aggregation](aggregation.md) gives its expression. [Project functions](functions.md)
gives `function` the return type declared by its logical contract.

### Scope

<a id="req-0465"></a>

**REQ-0465.** An `aggregate_expression` reduces records from one relation and
returns one value per group. The expression never changes the row count.
[Lookup and joins](lookup.md) joins
a right-side reduction to constructed rows. An output-row reduction broadcasts
under [REQ-0467](aggregation.md#req-0467). A grouped row template asks the expression for one value while [Execution lifecycle](../execution/lifecycle.md)
decides whether that candidate row is appended.

### Relations and identifiers

<a id="req-0466"></a>

**REQ-0466.** An identifier is `NAME` or `DATASET.NAME`. Name binding
[resolves each identifier](../specification/binding.md) in the same phase. A
reducer expression and predicate resolve each name the same way.

<a id="req-0467"></a>

**REQ-0467.** Every identifier in one expression must name one relation.
An aggregate is valid in exactly the following three contexts. The forms
must not be mixed; every other context is an error:

- **Qualified.** Every identifier names the same declared dataset relation.
  During column derivation the expression reduces that right side before the
  [Lookup and joins](lookup.md) join, even when the expression qualifier equals the current row
  template's input dataset. A scalar source qualified to the row
  template's input dataset reads one record. The aggregate keyword makes
  the same qualifier relational.
- **Unqualified.** Every identifier names a current-output column. The
  expression reduces constructed output rows within its `group_by` partition
  and broadcasts the result to every row in the partition.
- **Grouped input.** Every identifier is qualified
  to the input dataset of the enclosing grouped row template. The
  expression is a row derivation that reduces only the records of the
  current input group to one candidate-row value.

<a id="req-0468"></a>

**REQ-0468.** A single expression naming two datasets, or mixing a qualified
identifier with an unqualified one, is an error. A reduction is not a join. An
expression combining two dataset relations first binds each relation to a
column and then combines the results with `compute`. [Numeric computation](computation.md) admits a qualified
identifier only for a record selected by an [Lookup and joins](lookup.md) named intermediate. [Numeric computation](computation.md) still
rejects an arbitrary dataset-qualified identifier. Every join remains
under [Lookup and joins](lookup.md).

<a id="req-0469"></a>

**REQ-0469.** An ODM contextual reference is not available in this grammar. ODM
item identifiers carry further periods. Bind the reference with a structured
`source` first.

<a id="req-0470"></a>

**REQ-0470.** `group_by` follows the first two forms. An ordinary qualified
expression declares qualified right-side columns. Each column must also be an
output key. The reduction stays coarser than or equal to the
applicable keys [Lookup and joins](lookup.md) joins on. An unqualified expression declares
current-output columns and must declare at least one. A reduction over the
whole output is not registered: no example needs one. A grouped-row aggregate
declares no local `group_by`. The enclosing
`row.group_by` already fixes its current relation and keys.

### Filter scope

<a id="req-0471"></a>

**REQ-0471.** An aggregate `filter` selects records from its evaluation
context: right-side records for a qualified column derivation, constructed
output rows for an unqualified reduction, and current input-group records
for a grouped row derivation. [Predicates](predicates.md) defines predicate evaluation; filtering
preserves the order of the retained records under [REQ-0480](aggregation.md#req-0480).

### Derive step

<a id="req-1189"></a>

**REQ-1189.** A qualified aggregate may declare `derive` to bind per-record
intermediate variables before reduction. Each binding declares a `name`, a
`type`, and a `derivation`. Bindings evaluate once per record of the
aggregate's relation, in declaration order; each derivation reads the
record's fields and the values bound by earlier bindings. The reducer
expression names each bound variable by its unqualified `name`. A derive
binding is not an expression function: text becomes a number only through
the binding's declared `type` ([REQ-1190](aggregation.md#req-1190)), and a
date becomes an integer only through an operation such as
`to_epoch_day` in the binding's own derivation. The aggregate and numeric
computation grammars never parse text or dates themselves.

<a id="req-1190"></a>

**REQ-1190.** Each derive binding's evaluated value converts to its declared
`type` through [Types and conversion](../values/types.md)'s REQ-0009 and
REQ-0010. Text that is not convertible to the declared numeric type fails as
a conversion failure under [Types and conversion](../values/types.md)'s
REQ-0013, answered by a declared `missing` handler and fatal
otherwise. A missing value stays missing without attempting conversion.

<a id="req-1191"></a>

**REQ-1191.** The relation a derived aggregate reduces is the one relation
its derive bindings and filter name; naming two relations is an error, and a
reducer expression mixing a bound variable with a qualified identifier is an
error under [REQ-0468](aggregation.md#req-0468). `derive` is not available on
the unqualified output-row reduction or the grouped-input reduction: those
contexts reduce rows the specification already constructed.

### Row-relative range narrowing

<a id="req-0472"></a>

**REQ-0472.** A qualified aggregate may declare `between` to narrow right-side
records for each current row. `value` is a variable the current row can read.
`lower` and `upper` are qualified columns of the aggregate expression's one
right-side relation, and at least one is required. Every declared comparison is
inclusive: `lower <= value` and `value <= upper`. Omitting one bound makes the
match one-sided without excluding the stated endpoint.

<a id="req-0473"></a>

**REQ-0473.** The value and every stated bound must be mutually comparable under
[REQ-0005](../values/types.md#req-0005). A missing current-row value admits no right-side record. The aggregate
result is missing under the empty-group rule below. A right-side
record with a missing stated bound is ineligible. A missing cutoff never
reduces the unrestricted right side.

<a id="req-0474"></a>

**REQ-0474.** `between` is invalid on an unqualified or grouped-row aggregate:
neither has a separate right-side relation to narrow for each current row.

### Grammar

<a id="req-0475"></a>

**REQ-0475.** The grammar is:

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

`grammar/aggregate.yaml` is this grammar's single source. The block above is
its rendering. Its vocabulary closes the reducer table below. Its
cases record the text each implementation must accept or reject, how accepted
identifiers bind, and the parse each case produces. Repository validation and
the R implementation read that file. The grammar cannot drift without failure.

<a id="req-0476"></a>

**REQ-0476.** Precedence, associativity, and permitted `function` names follow
[Numeric computation](computation.md). Reducer and function names and `NULL` are case-insensitive. Identifiers
are case-sensitive.

<a id="req-0477"></a>

**REQ-0477.** Permitted reducers are exactly:

| Reducer | Result |
|---|---|
| `SUM(x)` | total of the non-missing values |
| `COUNT(x)` | how many values of `x` are non-missing |
| `COUNT(D.*)` | how many records the group contains |
| `MIN(x)` | smallest non-missing value |
| `MAX(x)` | largest non-missing value |
| `MEAN(x)` | arithmetic mean of the non-missing numeric values |
| `ONLY(x)` | value from the group's only record; more than one fails |

<a id="req-0478"></a>

**REQ-0478.** Any other reducer name, any window function or `OVER`, any
subquery, any `CASE`, any comparison or Boolean operator, any string literal,
and any host-language call are validation errors. A closed vocabulary makes
portability checkable. Any new reduction must amend this table.

<a id="req-0479"></a>

**REQ-0479.** For a group with at least one non-missing value, `MEAN(x)` is
evaluated as `SUM(x) / COUNT(x)` under this contract's `SUM` semantics and [Numeric computation](computation.md)'s
`/` semantics. The defined division fixes the result and failure behavior
across runtimes without a host language mean.

<a id="req-0480"></a>

**REQ-0480.** `SUM(x)` is a left fold of the non-missing argument values in
relation record order. The accumulator starts with the first such value. Each
later value is added with [Numeric computation](computation.md)'s `+` semantics. Implementations must not
reorder, reassociate, partition, or use a compensated or correctly rounded
summation. The `filter`, when present, removes records and keeps the order of
the records that remain. [Source ingestion](../storage/ingestion.md) defines stored-source record order. [Execution lifecycle](../execution/lifecycle.md)
defines constructed-output and grouped-input record order. `MEAN` uses the same
ordered `SUM`, followed by division by `COUNT`. `MEAN` inherits the
fold's binary64 rounding behavior.

<a id="req-0481"></a>

**REQ-0481.** `AVG` is not an alias; the portable reducer name is `MEAN`. A
median would have to fix its interpolation rule before two runtimes could
agree. No median is registered by default.

<a id="req-0482"></a>

**REQ-0482.** `ONLY` counts records, not non-missing values. An eligible group
with one record returns that record's value even when the value is missing. An
eligible group with more than one record fails rather than choosing by value or
record order. `ONLY` is the reduction for a grouped calculation that requires
one source record and must reject duplicates.

<a id="req-0483"></a>

**REQ-0483.** Reductions do not nest. A reduction argument must contain no
reduction, so `MAX(SUM(EX.EXDOSE))` is an error. Reducing at one key level and
reducing that result at another uses two specifications. The first artifact
names and validates the intermediate keys. The downstream specification
declares that stored artifact as an ordinary source under [Name binding](../specification/binding.md). Pipeline
orchestration supplies the execution and materialization boundary. The boundary
is not inferred from a source path.

<a id="req-0484"></a>

**REQ-0484.** `COUNT(D.*)` takes no other argument; in this contract, `D` is a
placeholder for the relation named by the expression's qualified identifiers
(for example, `COUNT(EX.*)`). `COUNT(D.*)` is the one reducer that names no
column and counts records where `COUNT(x)` counts values.

### The key rule

<a id="req-0485"></a>

**REQ-0485.** Every identifier must appear inside a reduction, unless it names a
`group_by` column. `SUM(a) / SUM(b)` is legal. `SUM(a) + b` is an error unless
`b` is grouped on. A value that varies within a group gives the expression no
single answer. Taking one record's value would depend on record order. For a
grouped-row aggregate, the enclosing `row.group_by` supplies the grouped
columns.

<a id="req-0486"></a>

**REQ-0486.** An identifier in `group_by` is constant within the group and
may be used directly, so `SUM(EX.EXDOSE) / EX.EXPLDOS` is legal exactly
when `EX.EXPLDOS` is declared in `group_by`.

### Types

<a id="req-0487"></a>

**REQ-0487.** A single reduction retains its result type. `COUNT` returns
  `int`; `MEAN` returns `float`. `SUM` retains its argument's numeric type.
  `MIN`, `MAX`, and `ONLY` retain the type they reduce, whatever that type is.

<a id="req-0488"></a>

**REQ-0488.** An expression using any operator or [Numeric computation](computation.md) function is numeric.
  Every reduction and grouped identifier in it must be numeric, and [Numeric values](../values/numbers.md)'s
  promotion rules give the result type.

<a id="req-0489"></a>

**REQ-0489.** `SUM` and `MEAN` require a numeric argument. `MIN` and `MAX`
  require mutually comparable values; a column mixing incomparable types is an
  error rather than an implementation-defined order. Their string order is
  [Text values](../values/text.md)'s. `COUNT` and `ONLY` accept any type.

<a id="req-0490"></a>

**REQ-0490.** [Types and conversion](../values/types.md) converts a completed derivation result, as it does for every
other expression. No implicit conversion happens inside this grammar.

### Missing values and empty groups

<a id="req-0491"></a>

**REQ-0491.** Inside a reduction's argument, `NULL` propagates under [Numeric computation](computation.md). A
record whose operand is missing contributes a missing value rather than a zero.
`SUM(EX.EXDOSE * EX.EXDUR)` skips a record missing either factor.

<a id="req-0492"></a>

**REQ-0492.** A reduction then ignores missing values. The table defines the
other results. The three target runtimes disagree:

| Condition | Result |
|---|---|
| No record in the group after `filter` | missing, as [Lookup and joins](lookup.md)'s absent match |
| Every value missing -- `SUM`, `MIN`, `MAX`, `MEAN` | missing, never zero |
| Every value missing -- `COUNT(x)` | `0`, because the records exist |
| No record in the group -- `COUNT(x)`, `COUNT(D.*)` | missing |
| No record in the group -- `ONLY(x)` | missing |
| One record whose value is missing -- `ONLY(x)` | missing |

<a id="req-0493"></a>

**REQ-0493.** An uncollected quantity is never reported as a measured zero, and
an absent record remains distinguishable from a collected missing value.

<a id="req-0494"></a>

**REQ-0494.** More than one record reaching `ONLY` is not a missing-value case.
It fails the current derivation and reports the group values and record count.

<a id="req-0495"></a>

**REQ-0495.** `MEAN` returns missing before its defined division when no
non-missing value remains. An all-missing group does not fail with division by
zero. Arithmetic over reduction results follows [Numeric computation](computation.md). A missing
reduction propagates through an operator. A formula that must yield missing
rather than fail says so with `NULLIF`.

### Failure conditions

<a id="req-0496"></a>

**REQ-0496.** [Numeric computation](computation.md)'s failure conditions apply to the arithmetic unchanged:
division by zero, `SQRT` of a negative argument, `LN` of a non-positive
argument, invalid `POWER`, and integer overflow each fail the run. [Types and conversion](../values/types.md)'s non-
finite normalization applies after every arithmetic or reduction result. `SUM`
fails on integer overflow under the same condition. Because `MEAN` is defined
by `SUM`, the same intermediate overflow fails even when the mathematical mean
would fit.

### Determinism

<a id="req-0497"></a>

**REQ-0497.** Evaluation must be deterministic and side-effect free. R and
Python must produce identical results for every example. [Numeric computation](computation.md)'s determinism
requirements apply unchanged, including that an implementation must not
reassociate or algebraically simplify a written expression.

<a id="req-0498"></a>

**REQ-0498.** A reduction does not sort the records. `SUM` and therefore `MEAN`
consume relation record order as specified above. `COUNT`, `MIN`, and `MAX` are
independent of that order, while `ONLY` accepts no group in which an order
could choose among records. A rule that needs one record chosen by value order
still uses a window or `multiple_matches`, where the value order is declared.

### Interface behavior

<a id="req-1088"></a>

**REQ-1088.** The `aggregate_class` interface has the following meanings. Shape, defaults, and
structural constraints come from its schema declaration.

| Field | Meaning |
| --- | --- |
| `aggregate_class.filter` | Predicate selecting records before reduction. |
| `aggregate_class.between` | Current-row value matched inclusively against one or two columns of a qualified right-side relation. |
| `aggregate_class.group_by` | Grouping keys of an ordinary right-side or output-row reduction; omit when the enclosing grouped row owns the keys. |
| `aggregate_class.key` | Dataset columns matched against key_base; omit to match on the applicable output keys ([REQ-0150](lookup.md#req-0150)) when the expression reads a qualified dataset relation. |
| `aggregate_class.key_base` | Current-row variables paired by position with key; omit when they name the same columns as key. Must not repeat the key names ([REQ-0155](lookup.md#req-0155)). |
| `aggregate_class.derive` | Per-record intermediate variable bindings evaluated before reduction ([REQ-1189](aggregation.md#req-1189)). |
| `aggregate_class.expr` | Closed reducer expression over the records of one relation. |

<a id="req-1089"></a>

**REQ-1089.** The `aggregate_expression` interface has the following meanings. Shape, defaults, and
structural constraints come from its schema declaration.

| Field | Meaning |
| --- | --- |
| `aggregate_expression` | Reducer expression in the portable grammar defined by [Aggregation](aggregation.md). |

<a id="req-1090"></a>

**REQ-1090.** The `aggregate_between_class` interface has the following meanings. Shape, defaults, and
structural constraints come from its schema declaration.

| Field | Meaning |
| --- | --- |
| `aggregate_between_class.value` | Value read from the current row. |
| `aggregate_between_class.lower` | Qualified right-side variable serving as an inclusive lower bound. |
| `aggregate_between_class.upper` | Qualified right-side variable serving as an inclusive upper bound. |

<a id="req-1091"></a>

**REQ-1091.** The `module` interface has the following meanings. Shape, defaults, and
structural constraints come from its schema declaration.

| Field | Meaning |
| --- | --- |
| `Scope` | [Aggregation](aggregation.md) defines the reducer grammar, the results it pins, and its failures. |

<a id="req-1192"></a>

**REQ-1192.** The `derive_binding_class` interface has the following meanings. Shape, defaults, and
structural constraints come from its schema declaration.

| Field | Meaning |
| --- | --- |
| `derive_binding_class.name` | Intermediate variable the aggregate's reducer expression names. |
| `derive_binding_class.type` | Declared type; the per-record value converts to it under [REQ-1190](aggregation.md#req-1190). |
| `derive_binding_class.derivation` | Per-record derivation over the relation's fields and earlier bindings. |

<a id="req-1092"></a>

**REQ-1092.** The `expressions.aggregate` interface has the following meanings. Shape, defaults, and
structural constraints come from its schema declaration.

| Field | Meaning |
| --- | --- |
| `Scope` | Reduces one relation or the current input group to one value under [Aggregation](aggregation.md). |

## Error conditions

<a id="req-0329"></a>

**REQ-0329.** An aggregate outside its three permitted contexts: fail.

<a id="req-0330"></a>

**REQ-0330.** An aggregate declaring `between` outside the qualified dataset
context: fail.

<a id="req-0331"></a>

**REQ-0331.** A grouped-row aggregate naming a dataset other than its
row template's input dataset or declaring its own `group_by`: fail.

<a id="req-0332"></a>

**REQ-0332.** An `aggregate` expression that violates [Aggregation](aggregation.md): fail.

<a id="req-0499"></a>

**REQ-0499.** An `aggregate_expression` that does not parse under this grammar:
fail.

<a id="req-0500"></a>

**REQ-0500.** A reducer name outside the table, or one called with a
prohibited argument count: fail.

<a id="req-0501"></a>

**REQ-0501.** `ONLY` receiving more than one
record after its filter: fail, reporting the enclosing row, group values, and
record count.

<a id="req-0502"></a>

**REQ-0502.** A nested reduction: fail, reporting the outer and
inner reducers.

<a id="req-0503"></a>

**REQ-0503.** An identifier outside a reduction that is not a
`group_by` column: fail, reporting the identifier.

<a id="req-0504"></a>

**REQ-0504.** An expression
naming more than one dataset, or mixing a qualified identifier with an
unqualified one: fail.

<a id="req-0505"></a>

**REQ-0505.** A `COUNT(D.*)` whose dataset is not the
expression's relation: fail.

<a id="req-0506"></a>

**REQ-0506.** An ODM contextual reference: fail.

<a id="req-0507"></a>

**REQ-0507.** A qualified `group_by` column that is not an output key, or an
unqualified expression with no `group_by`: fail.

<a id="req-0508"></a>

**REQ-0508.** A grouped-row
aggregate declaring its own `group_by`, naming an identifier outside its row
template's input dataset, or being used by an ungrouped row template: fail.

<a id="req-0509"></a>

**REQ-0509.** A `between` on an unqualified or grouped-row aggregate, declaring
neither bound, naming a bound outside the qualified relation, or using
incomparable operands: fail.

<a id="req-0510"></a>

**REQ-0510.** `SUM` or `MEAN` over a non-numeric
argument, or arithmetic over a non-numeric reduction or grouped identifier:
fail.

<a id="req-0511"></a>

**REQ-0511.** `MIN` or `MAX` over incomparable values: fail.

<a id="req-0512"></a>

**REQ-0512.** A window, `CASE`, comparison, Boolean, string, subquery, or host construct:
fail.

<a id="req-0513"></a>

**REQ-0513.** Any [Numeric computation](computation.md) failure condition reached through the arithmetic:
fail, reporting the expression and the column that failed.

## Conformance examples

Representative specifications, input data, and expected outcomes:

- [negative-dose-intensity](../../benchmarks/negative-dose-intensity/README.md).
- [negative-adlb-duplicate-wbc](../../benchmarks/negative-adlb-duplicate-wbc/README.md).
- [negative-sum-non-numeric](../../benchmarks/negative-sum-non-numeric/README.md).

The [execution manifest](../../benchmarks/execution-manifest.yaml) records
which fixtures execute. Grammar contracts additionally replay their shared
vectors. Static validation does not establish runtime parity.

## Rationale

Reduce eligible records within one of three permitted key scopes. One contract
avoids a second policy.
