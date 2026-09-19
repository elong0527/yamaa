---
id: R007
title: Expression Registry
status: normative
applies_to: [expression, expressions, schema_expression]
---

# Expression registry

## Intent

Give every derivation keyword a closed schema and defined dispatch semantics.
Do not use a generic operation argument bag. Built-in expressions are
portable. `function` is the explicit project-environment extension point.

## Boundaries

This rule owns registration, the nesting policy, evaluation kinds, ordering
terms, and cross-operation type compatibility. Behavior specific to one
operation is documented beside its registry entry. Cross-cutting behavior stays
in its owning rule: R002 and R003 for source binding and joins, R008 for local
handlers, R010 for `compute`, R011 for column types, R012 for string templates,
R013 for aggregate reduction, R014 for the type a source field carries, R015
for a record selected once and read by several columns, R016 for dates and
datetimes, R018 for project functions, and R004 for predicates.
R019 owns string values, casing, equality, and order.

## Registration

**R007-1.** `schema_expression_*.yaml` and `schema_function.yaml` contribute
entries to the `expressions` registry under R006. `schema_derivation.yaml`
exposes that registry as the `expression` type.

**R007-2.** Each registered keyword owns all its inputs, options, grouping,
local error handlers, and operation-local semantics. Adding a keyword
requires one complete registry entry. Unknown keywords and unknown payload
fields fail validation.

## Nesting policy

**R007-3.** `source` and `literal` are expression leaves. Every other
expression names its input variables directly, except in the following fields
whose declared type contains `expression`. Each is evaluated recursively and
nests because selecting or composing expressions is the field's purpose:

- `case` items: `case` selects among expressions, so each `then` and the
  trailing `otherwise` nests an expression.
- `str_concat.sources`: concatenation places literals beside sources.

**R007-4.** `derivation` and `handled_expression_class.value` also contain
`expression`, but they hold a derivation's own top-level expression rather
than nest one inside an operation, so this policy does not restrict them.

**R007-5.** Fields typed `numeric_expression`, `string_template`, and
`aggregate_expression` are leaves whose identifiers R010, R012, and R013
resolve. Plain strings are values unless their schema field is typed as
`variable`, `function_arg`, `sql`, or `string_template`. R018 closes
`function_arg`: a string is a variable, while string, date, and datetime
literals use their explicit tagged leaf forms.

## Evaluation kinds

**R007-6.** Scalar expressions return one value per row. Window expressions
partition constructed output rows by their `window` specification's
`group_by` and preserve row count. Omitting `group_by` creates one
partition. Within a declared group, missing values equal other missing
values. Rows with equal present values and equal missing group positions
share one partition. A window partition is the KRC
section: a group of rows.

**R007-7.** A window whose `window` declares `filter` still preserves row
count: an excluded row receives missing rather than being dropped. A window
that reads another row of its partition returns missing when that row does
not exist, the same result as for a neighbouring row with a missing value.

**R007-8.** `aggregate` is the only aggregate expression. R013 defines its
grammar, the reducers it permits, and what each returns. This rule fixes
where it may be used. It is valid in exactly three contexts. Context 1: its
identifiers are qualified to one declared dataset relation during column
derivation. It then reduces that right side before the R003 join. R003
defines the join. The qualifier may equal the current row template's input
dataset because an aggregate reads the relation rather than the
scalar input record.

**R007-9.** Context 2: its identifiers are unqualified. It then declares
`group_by`, reduces constructed output rows within each partition, and
broadcasts the result to each row.

**R007-10.** Context 3: it is a row derivation of a grouped row template
and every identifier is qualified to that template's input
dataset. It reduces the records of the current input group to one
candidate-row value. The enclosing `row.group_by` owns the keys, so
the aggregate itself omits `group_by`.

**R007-11.** Any other aggregate context is an error. A `filter` narrows the
records the owning expression works in: right-side records for
context 1, and constructed output rows for a window or for context 2, and
current input-group records for context 3. `between` is valid only in
context 1. It narrows those right-side records separately for each current
row under R013.

## Ordering

**R007-12.** Every field typed `list[order_by_term]` is a list of order
terms, whichever operation declares it. An order term is either a bare
variable or a mapping declaring `variable`, `direction`, and `nulls`. The
bare form is an R006 shorthand union, so a bare variable means
`{variable: X, direction: asc, nulls: last}`.

**R007-13.** `direction` is `asc` or `desc` and defaults to `asc`.

**R007-14.** `nulls` is `last` or `first` and defaults to `last`. It states
where missing values sit among the non-missing ones for that term.

**R007-15.** `nulls` does not flip with `direction`. `last` means last under
`asc` and last under `desc`. SQL engines disagree on this default, so an
implementation must apply the declared placement rather than inherit its
engine's.

**R007-16.** Terms apply in order, each with its own direction and
placement. Records equal on every term preserve row-template order and then
base-record order. The result is total: ordering has no
undefined case and a row's neighbours are determined.

**R007-17.** Non-missing values use the order their type owns: numeric order
under R010, text order under R019, and chronological order for `date` and
`datetime` under R016.

**R007-18.** The tie-break settles positions, not equality. `row_number`,
`row_value`, `previous_non_missing`, and right-side selection read the
positions themselves, so a tie changes which row they reach. `rank` compares
only the declared terms. Records equal on all declared terms receive a
single number rather than the distinct numbers their positions would give.
The `competition` method leaves the positions occupied by a tie out of the
subsequent numbers. The `dense` method numbers distinct values
consecutively. A specification that wants a tie broken declares the term
that breaks it, whichever method it uses.

## Type behavior

**R007-19.** No implicit conversion occurs between named operation inputs.
R005 converts only the completed derivation result. Inputs must therefore
have compatible runtime types.

**R007-20.** `mapping` requires a string source because dictionary keys are
strings.

**R007-21.** `mapping_from` requires each source and its positionally
corresponding dictionary key column to have the same comparable type.

**R007-22.** `cut` requires a numeric source.

**R007-23.** `compute` requires every identifier in its expression to be
numeric.

**R007-24.** `str_extract`, `str_concat`, `str_template`, `str_upper`, and
`str_lower` require string sources.

**R007-25.** `date_diff`, `study_day`, `date_impute`, `date_precision`, and
`to_date` state their own input types in R016.

**R007-26.** `greatest` and `least` require mutually comparable `sources`.

**R007-27.** `row_value` requires an integer `offset`; it and
`previous_non_missing` accept any `source` type and perform no coercion.

**R007-28.** Window ordering requires mutually comparable values. One order
term names one variable, and a variable has exactly one type -- R014 gives
it to a source field and R011 to a declared column -- so the values a term
compares are of one type by construction and ordering has no incomparable
case. An expression naming several variables, as `greatest` and `least` do,
is where comparability is a requirement rather than a consequence.

**R007-29.** `aggregate` states its own input types in R013.

**R007-30.** `function` states its exact argument and result types in R018.

**R007-31.** Comparability is a property of the runtime type. `int` and
`float` are mutually comparable, because R010 promotes them. Every other
type is comparable only with itself. Collected precision, which R016
defines, is not a runtime type and so takes no part in comparability. Two
temporal values of one type are comparable whatever precision each carries.
A comparable type therefore satisfies any input requiring mutually
comparable values -- `greatest` and `least`, `mapping_from` key pairing, an
`order_by` term, and R013's `MIN` and `MAX` -- while a `sources` list or one
ordering term mixing two types is the incompatible-input error below rather
than a comparison over a coerced operand. Each owning rule defines the order
its type takes.

**R007-32.** `source` retains its source type, which R014 defines.
`literal` retains its YAML scalar type after R011's non-finite
normalization. `cut`, `str_extract`, `str_concat`, `str_template`,
`str_upper`, and `str_lower` return strings. R019 owns the casing and
text-preservation behavior of those string operations. R022 owns
`str_extract`'s pattern and the match it keeps. `compute` returns the
numeric type its expression promotes to under R010.

**R007-33.** `row_number` and `rank` return integers. The temporal operations
return the types R016 gives them. `baseline_flag` returns a string. Mapping,
conditional, coalescing, extreme, baseline value, offset row, and
previous-non-missing expressions retain the selected value type. A
selected temporal value carries its collected precision unchanged.

**R007-34.** `aggregate` returns the type R013 gives its expression. R018
gives `function` the return type declared by its logical contract.

## Operation definitions

**R007-35.** Each operation is documented where it is registered in
`schema_expression_*.yaml` or `schema_function.yaml`. An inline comment
states the operation result. Descriptor `description` fields explain
parameters. These definitions are authoritative for operation-local behavior
and do not affect schema validation.

## Rationale

Keeping each keyword's inputs, options, grouping, handlers, and semantics
inside its registry entry keeps the language checkable. No generic argument bag
can drift between implementations. Nesting is allowed only where selecting or
composing expressions is the field's purpose. An operation cannot silently
become a second expression language. The three aggregate contexts match the
language's three key scopes: a joined relation, a constructed partition, and an
input group. The rule fixes order-term defaults. SQL engine disagreement about
null placement must not change results. Runtime types make an order term
compare one type by construction. Multi-variable expressions are the only
constructs that require stated comparability.

## Errors

**R007-36.** An unregistered expression keyword or invalid payload: fail
under R006.

**R007-37.** A semantic constraint in an operation definition or applicable
rule that is not satisfied: fail.

**R007-38.** An input with an incompatible runtime type: fail.

**R007-39.** A `sources` list or an ordering term mixing two runtime types
that are not mutually comparable: fail rather than convert an operand.

**R007-40.** A scalar or window expression that changes row count: fail
under R001, which owns the phase invariant.

**R007-41.** A window expression used during row construction: fail.

**R007-42.** A `window.filter` that is not a Boolean predicate over
current-output columns: fail.

**R007-43.** A `row_value` whose `offset` is zero: fail. The current row's
own value is `source`, and a window must not be a second spelling of it.

**R007-44.** An aggregate outside its three permitted contexts: fail.

**R007-45.** An aggregate declaring `between` outside the qualified dataset
context: fail.

**R007-46.** A grouped-row aggregate naming a dataset other than its
row template's input dataset or declaring its own `group_by`: fail.

**R007-47.** An `aggregate` expression that violates R013: fail.

**R007-48.** `mapping_from` whose `source` and `key` lists differ in length:
fail.

**R007-49.** An unhandled local missing, mapping, or extraction condition:
fail under R008.

**R007-50.** A `compute` expression that violates R010: fail.

**R007-51.** A `str_template` expression that violates R012: fail.

**R007-52.** A temporal value or operation that violates R016: fail.

**R007-53.** A project function that violates its environment, contract,
binding, or result requirements: fail under R018.

**R007-54.** A `case` is a non-empty list of `case_item_class`. Every item
declares `when` and `then` except that one item may declare `otherwise`
instead; the `otherwise` item, when present, is the last item. A `case`
with no `when`/`then` item, more than one `otherwise` item, or an
`otherwise` item in any other position: fail.

**R007-55.** `row_number`, `rank`, `row_value`, and `previous_non_missing`
require `window.order_by`: without a declared order the window has no
positions to number or to move along. Omitting it is a validation error.

**R007-56.** `baseline_flag` and `baseline_value` do not take
`window.order_by`: they locate the baseline row by date and flag, not by a
declared order. Declaring it is a validation error rather than silently
ignored.
