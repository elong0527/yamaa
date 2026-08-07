---
id: R007
title: Expression Registry
status: normative
applies_to: [expression, expressions, schema_expression]
depends_on: [R004, R006]
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

Each registered keyword owns all its inputs, options, grouping, and local error
handlers. Adding a keyword requires one registry entry and normative semantics.
Unknown keywords and unknown payload fields fail schema validation.

`source` and `literal` are expression leaves. Other expressions name their
input variables directly unless a field is explicitly typed as `expression`.
The latter is reserved for constructs whose purpose requires nested values:
`case`, function arguments, and final overrides. Plain strings are values unless
their schema field is typed as `variable`, `function_arg`, or `sql`. A string in
`function_arg` is a variable; a string literal uses the `literal` expression.

## Evaluation kinds

Scalar expressions return one value per row. Window expressions partition
constructed output rows by their local `group_by` and preserve row count.
Omitting `group_by` creates one partition.

`min` and `max` are aggregates. They are valid in exactly two contexts:

1. Their `source` is a qualified cross-dataset source, optionally narrowed by
   `filter`, which reduces the right side before the R003 join.
2. They declare `group_by`, reduce constructed output rows within each
   partition, and broadcast the result to each row.

Any other aggregate context is an error.

## Type behavior

No implicit conversion occurs between named operation inputs. R005 converts
only the completed derivation result. Inputs must therefore have compatible
runtime types:

- `mapping` requires a string source because dictionary keys are strings;
- `mapping_from` requires the source and dictionary key column to have the same
  comparable type;
- `cut`, `multiply`, `add`, `subtract`, and `percent_change` require numeric
  inputs;
- `str_extract` requires a string source;
- `date_diff` requires compatible date or datetime inputs;
- `min`, `max`, and window ordering require mutually comparable values.

`source` retains its source type and `literal` retains its YAML scalar type.
`cut` and `str_extract` return strings. `multiply`, `add`, `subtract`, and
`percent_change` return floats. `date_diff` and `row_number` return integers.
`baseline_flag` returns a string. Mapping, conditional, coalescing, baseline
value, and aggregate expressions retain the selected or aggregated value type.
The `function` expression retains the type returned by the project function.

## Registered semantics

### Leaves

- `source` resolves its variable under R002 and R003. Structured binding fields
  are governed by R003 and R008.
- `literal` returns its scalar value exactly as resolved by R006.

### Mapping and string expressions

- `mapping` reads `source` and returns `dict[value]`. A missing source value
  returns `missing` when supplied. A non-missing value absent from `dict`
  returns `unmapped` when supplied. Otherwise either condition fails. When
  `case_sensitive` is false, fold ASCII `A`-`Z` to `a`-`z` in the input and
  dictionary keys and leave every other code point unchanged. Dictionary keys
  that collide after folding are an error.
- `mapping_from` reads `source`, matches it against column `key` in the declared
  `dataset`, and returns column `value`. The dictionary must be unique on `key`.
  A missing source returns `missing` when supplied. A non-missing value with no
  match returns `unmapped` when supplied. Otherwise either condition fails.
- `cut` assigns one of `len(breaks) + 1` labels using ascending breaks. Labels
  must have that exact length. With `right: false`, intervals are left-closed
  and right-open. Missing input returns `missing` when supplied, or fails.
- `str_extract` matches `pattern` as an ECMAScript regular expression and
  returns capture group `group`, where zero is the complete match. Missing input
  returns `missing` when supplied. A non-missing input with no match returns
  `no_match` when supplied. Otherwise either condition fails.

### Numeric and conditional expressions

- `multiply` returns `source * factor`.
- `add` returns `source + addend`.
- `subtract` returns `minuend - subtrahend`.
- `percent_change` returns `100 * (value - base) / base`; a zero or missing
  base returns missing.
- `coalesce` returns the first non-missing variable in `sources`. If all are
  missing, it returns the literal `default` when supplied, or missing.
- `case` evaluates branches in order and returns the `then` expression of the
  first `TRUE` predicate. It returns `else` when no branch matches, or missing
  when `else` is absent.

### Date expression

- `date_diff` returns the count of whole `unit` intervals from `start` to `end`,
  negative when `end` precedes `start`. It excludes the start point. Permitted
  units are `day`, `week`, `month`, and `year`.

### Window expressions

- `row_number` numbers rows from one within each partition, ascending by the
  variables in `order_by`. Ties preserve row-template order and then
  base-record order.
- `baseline_flag` returns `Y` for the row with the latest non-missing `date` at
  or before `reference_date`, and missing elsewhere. Ties are errors.
- `baseline_value` copies the `value` from the row whose `flag` is `Y` to every
  row in the partition. More than one flagged row is an error.

### Aggregate expressions

- `min` returns the smallest non-missing `source`, or missing if all are missing.
- `max` returns the largest non-missing `source`, or missing if all are missing.

### Project function expression

- `function` resolves direct string arguments as variables, evaluates argument
  expressions, retains direct numeric, Boolean, and null literals, resolves
  `name` in the project's global execution environment, and invokes it with the
  named arguments. String literals use the `literal` expression. Its logical
  result is one scalar value per current row. An implementation may vectorize
  calls only when that is equivalent to the logical row-wise result. Function
  availability, signature, and package setup belong to the project environment.

String collation for ordering and aggregation follows R004 and remains
unresolved while that rule is draft.

## Errors

- An unregistered expression keyword or invalid payload: fail under R006.
- A semantic constraint stated above that is not satisfied: fail.
- An input with an incompatible runtime type: fail.
- A scalar or window expression that changes row count: fail.
- A window expression used during row construction: fail.
- An aggregate outside its two permitted contexts: fail.
- An unhandled local missing, mapping, or extraction condition: fail.
- An unresolved function, failed function call, or non-scalar function result:
  fail with the function name and original runtime context.
