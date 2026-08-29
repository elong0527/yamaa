---
id: R007
title: Expression Registry
status: normative
applies_to: [expression, expressions, schema_expression]
depends_on: [R001, R002, R003, R004, R005, R006, R008, R010, R011, R012]
---

# Expression registry

## Intent

Give every derivation keyword a closed, self-contained schema and defined
dispatch semantics without a generic operation argument bag. Built-in
expressions are portable; `function` is the explicit project-environment
extension point.

## Boundaries

This rule owns registration, the nesting policy, evaluation kinds, ordering
terms, and cross-operation type compatibility. Behavior specific to one
operation is documented beside its registry entry. Cross-cutting behavior stays
in its owning rule: R002 and R003 for source binding and joins, R008 for local
handlers, R010 for `compute`, R011 for column types, R012 for string templates,
and R004 for predicates.

## Registration

`schema_expression_*.yaml` and `schema_function.yaml` contribute entries to the
`expressions` registry under R006. `schema_derivation.yaml` exposes that
registry as the `expression` type.

Each registered keyword owns all its inputs, options, grouping, local error
handlers, and operation-local semantics. Adding a keyword requires one complete
registry entry. Unknown keywords and unknown payload fields fail validation.

## Nesting policy

`source` and `literal` are expression leaves. Every other expression names its
input variables directly, except in the following fields whose declared type
contains `expression`. Each is evaluated recursively and nests because
selecting or composing expressions is the field's purpose:

| Field | Why it nests |
|---|---|
| `case.branches[].then` and `case.otherwise` | Selecting among expressions is what `case` does |
| `str_concat.sources` | Concatenation places literals beside sources |
| `function.args` entries | A call site may pass a computed argument |
| `override[].value` | A final correction may select any expression |

`derivation` and `handled_expression_class.value` also contain `expression`,
but they hold a derivation's own top-level expression rather than nest one
inside an operation, so this policy does not restrict them.

Fields typed `numeric_expression` and `string_template` are leaves whose
identifiers R010 and R012 resolve. Plain strings are values unless their schema
field is typed as `variable`, `function_arg`, `sql`, or `string_template`. A
string in `function_arg` is a variable; a string literal uses the `literal`
expression.

## Evaluation kinds

Scalar expressions return one value per row. Window expressions partition
constructed output rows by their local `group_by` and preserve row count.
Omitting `group_by` creates one partition. A window that declares `filter`
still preserves row count: an excluded row receives missing rather than being
dropped. A window that reads another row of its partition returns missing when
that row does not exist, which is the same result as a neighbouring row whose
value is missing.

`min`, `max`, `sum`, and `count` are aggregates. They are valid in exactly two
contexts:

1. Their `source` is a qualified cross-dataset source. They then reduce the
   right side before the R003 join, which R003 defines.
2. They declare `group_by`, reduce constructed output rows within each
   partition, and broadcast the result to each row.

Any other aggregate context is an error. A `filter` narrows the records the
owning expression already works in: right-side records for context 1, and
constructed output rows for a window or for context 2.

## Ordering

Every field typed `list[order_by_term]` is a list of order terms, whichever
operation declares it. An order term is either a bare variable or a mapping
declaring `variable`, `direction`, and `nulls`. The bare form is an R006
shorthand union, so a bare variable means
`{variable: X, direction: asc, nulls: last}`.

- `direction` is `asc` or `desc` and defaults to `asc`.
- `nulls` is `last` or `first` and defaults to `last`. It states where missing
  values sit among the non-missing ones for that term.

**`nulls` does not flip with `direction`.** `last` means last under `asc` and
last under `desc`. SQL engines disagree on this default, so an implementation
must apply the declared placement rather than inherit its engine's.

Terms apply in order, each with its own direction and placement. Records equal
on every term preserve row-template order and then base-record order, which
makes the result total, so ordering has no undefined case and a row's
neighbours are determined.

That tie-break settles positions, not equality. `row_number`, `row_value`, and
right-side selection read the positions themselves, so a tie changes which row
they reach. `rank` and `dense_rank` compare only the declared terms, so records
equal on every one of them receive a single number rather than the distinct
numbers their positions would give. A specification that wants a tie broken
declares the term that breaks it, which is the same statement in both cases.

## Type behavior

No implicit conversion occurs between named operation inputs. R005 converts
only the completed derivation result. Inputs must therefore have compatible
runtime types:

- `mapping` requires a string source because dictionary keys are strings;
- `mapping_from` requires each source and its positionally corresponding
  dictionary key column to have the same comparable type;
- `cut` requires a numeric source;
- `compute` requires every identifier in its expression to be numeric;
- `str_extract`, `str_concat`, `str_template`, `str_upper`, and `str_lower`
  require string sources;
- `date_diff` and `study_day` require `date` inputs; R011 declares no datetime
  type;
- `date_impute` requires a string source, because a partial date is text under
  R011 until it is completed, and integer `month` and `day` within the calendar
  ranges its registration states;
- `greatest` and `least` require mutually comparable `sources`;
- `row_value` requires an integer `offset` and accepts any `source` type;
- `sum` requires a numeric source; `count` accepts any source type;
- `min`, `max`, and window ordering require mutually comparable values. Every
  record's value for one order term must be comparable with every other, so a
  term whose column mixes incomparable types is an error rather than an
  implementation-defined order.

`source` retains its source type, which R013 defines, and `literal` retains
its YAML scalar type.
`cut`, `str_extract`, `str_concat`, `str_template`, `str_upper`, and
`str_lower` return strings. `compute` returns the numeric type its expression
promotes to under R010.
`date_diff`, `study_day`, `row_number`, `rank`, `dense_rank`, and `count`
return integers.
`study_day` never returns zero. `date_impute` returns a `date`.
`baseline_flag` returns a string. Mapping, conditional, coalescing, extreme,
baseline value, offset row, and aggregate expressions retain the selected or
aggregated value type; `sum` retains the numeric type of its source and fails
on integer overflow, as R010 does. The `function` expression retains the type
returned by the project function.

## Operation definitions

Each operation is documented where it is registered in
`schema_expression_*.yaml` or `schema_function.yaml`. An inline comment states
the operation's result, and descriptor `description` fields explain its
parameters. These adjacent definitions are authoritative for operation-local
behavior and do not affect schema validation.

## Errors

- An unregistered expression keyword or invalid payload: fail under R006.
- A semantic constraint in an operation definition or applicable rule that is
  not satisfied: fail.
- An input with an incompatible runtime type: fail.
- A scalar or window expression that changes row count: fail under R001, which
  owns the phase invariant.
- A window expression used during row construction: fail.
- A window `filter` that is not a Boolean predicate over current-output
  columns: fail.
- A `row_value` whose `offset` is zero: fail. The current row's own value is
  `source`, and a window must not be a second spelling of it.
- An aggregate outside its two permitted contexts: fail.
- `mapping_from` whose `source` and `key` lists differ in length: fail.
- `date_impute` whose `month` or `day` is outside the calendar range, or whose
  completed value is not a real calendar date: fail.
- An unhandled local missing, mapping, or extraction condition: fail under
  R008.
- A `compute` expression that violates R010: fail.
- A `str_template` expression that violates R012: fail.
- An unresolved function, failed function call, or non-scalar function result:
  fail with the function name and original runtime context.
