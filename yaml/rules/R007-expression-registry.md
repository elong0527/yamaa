---
id: R007
title: Expression Registry
status: normative
applies_to: [expression, expressions, schema_expression]
depends_on: [R004, R006, R010]
---

# Expression registry

## Intent

Give every derivation keyword a closed, self-contained schema and defined
dispatch semantics without a generic operation argument bag. Built-in
expressions are portable; `function` is the explicit project-environment
extension point.

## Registration

`schema_expression_*.yaml` and `schema_function.yaml` contribute entries to the
`expressions` registry under R006. `schema_derivation.yaml` exposes that
registry as the `expression` type.

Each registered keyword owns all its inputs, options, grouping, local error
handlers, and operation-local semantics. Adding a keyword requires one complete
registry entry. Unknown keywords and unknown payload fields fail validation.

`source` and `literal` are expression leaves. Other expressions name their
input variables directly unless a field is explicitly typed as `expression`,
or typed as `numeric_expression`, whose identifiers R010 resolves against
current-output columns.
The latter is reserved for constructs whose purpose requires nested values:
`case`, function arguments, and final overrides. Plain strings are values unless
their schema field is typed as `variable`, `function_arg`, or `sql`. A string in
`function_arg` is a variable; a string literal uses the `literal` expression.

## Evaluation kinds

Scalar expressions return one value per row. Window expressions partition
constructed output rows by their local `group_by` and preserve row count.
Omitting `group_by` creates one partition. A window that declares `filter`
still preserves row count: an excluded row receives missing rather than being
dropped.

`min` and `max` are aggregates. They are valid in exactly two contexts:

1. Their `source` is a qualified cross-dataset source, optionally narrowed by
   `filter`, which reduces the right side before the R003 join.
2. They declare `group_by`, reduce constructed output rows within each
   partition, and broadcast the result to each row.

Any other aggregate context is an error.

## Ordering

`row_number.order_by` and `multiple_matches.order_by` are lists of order terms.
An order term is either a bare variable or a mapping declaring `variable`,
`direction`, and `nulls`. A bare variable means
`{variable: X, direction: asc, nulls: last}`, so an existing specification keeps
its meaning.

- `direction` is `asc` or `desc` and defaults to `asc`.
- `nulls` is `last` or `first` and defaults to `last`. It states where missing
  values sit among the non-missing ones for that term.

**`nulls` does not flip with `direction`.** `last` means last under `asc` and
last under `desc`. SQL engines disagree here — PostgreSQL places nulls last
under `asc` and first under `desc`, while SQLite and MySQL place them first
under `asc` — so an implementation must apply the declared placement rather
than inherit its engine's default.

Terms apply in order, each with its own direction and placement. Records equal
on every term preserve row-template order and then base-record order, which
makes the result total.

Ordering therefore has no undefined case. A specification no longer needs a
negated companion column to express a descending preference, and it no longer
needs to be built so that two records with a missing sort key cannot meet.

## Type behavior

No implicit conversion occurs between named operation inputs. R005 converts
only the completed derivation result. Inputs must therefore have compatible
runtime types:

- `mapping` requires a string source because dictionary keys are strings;
- `mapping_from` requires each source and its positionally corresponding
  dictionary key column to have the same comparable type;
- `cut` requires a numeric source;
- `compute` requires every identifier in its expression to be numeric;
- `str_extract`, `str_concat`, `str_upper`, and `str_lower` require string sources;
- `date_diff` and `study_day` require `date` inputs; R011 declares no datetime type;
- `date_impute` requires a string source, because a partial date is text under
  R011 until it is completed, and integer `month` and `day` within the calendar
  ranges its registration states;
- `greatest` and `least` require mutually comparable `sources`;
- `min`, `max`, and window ordering require mutually comparable values. Every
  record's value for one order term must be comparable with every other, so a
  term whose column mixes incomparable types is an error rather than an
  implementation-defined order.

`source` retains its source type and `literal` retains its YAML scalar type.
`cut`, `str_extract`, `str_concat`, `str_upper`, and `str_lower` return strings.
`compute` returns the numeric type its expression promotes to under R010.
`date_diff`, `study_day`, and `row_number` return integers. `study_day` never
returns zero. `date_impute` returns a `date`.
`baseline_flag` returns a string. Mapping, conditional, coalescing, extreme,
baseline value, and aggregate expressions retain the selected or aggregated
value type.
The `function` expression retains the type returned by the project function.

## Operation definitions

Each operation is documented where it is registered in
`schema_expression_*.yaml` or `schema_function.yaml`. An inline comment states
the operation's result, and descriptor `description` fields explain its
parameters. These adjacent definitions are authoritative for operation-local
behavior and do not affect schema validation.

Cross-cutting behavior remains in rules: R002 and R003 govern source binding,
R008 governs local handlers, and R010 governs `compute`. Predicate evaluation
and string collation follow R004; collation remains unresolved while that rule
is draft.

## Errors

- An unregistered expression keyword or invalid payload: fail under R006.
- A semantic constraint in an operation definition or applicable rule that is
  not satisfied: fail.
- An input with an incompatible runtime type: fail.
- A scalar or window expression that changes row count: fail under R001, which
  owns the phase invariant.
- A window expression used during row construction: fail.
- A `row_number` filter that is not a Boolean predicate over current-output
  columns: fail.
- An aggregate outside its two permitted contexts: fail.
- `mapping_from` whose `source` and `key` lists differ in length: fail.
- `date_impute` whose `month` or `day` is outside the calendar range, or whose
  completed value is not a real calendar date: fail.
- An unhandled local missing, mapping, or extraction condition: fail.
- A `compute` expression that violates R010: fail.
- An unresolved function, failed function call, or non-scalar function result:
  fail with the function name and original runtime context.
