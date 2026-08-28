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

Each registered keyword owns all its inputs, options, grouping, and local error
handlers. Adding a keyword requires one registry entry and normative semantics.
Unknown keywords and unknown payload fields fail schema validation.

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
- `date_diff` requires `date` inputs; R011 declares no datetime type;
- `greatest` and `least` require mutually comparable `sources`;
- `min`, `max`, and window ordering require mutually comparable values. Every
  record's value for one order term must be comparable with every other, so a
  term whose column mixes incomparable types is an error rather than an
  implementation-defined order.

`source` retains its source type and `literal` retains its YAML scalar type.
`cut`, `str_extract`, `str_concat`, `str_upper`, and `str_lower` return strings.
`compute` returns the numeric type its expression promotes to under R010. `date_diff` and `row_number` return integers.
`baseline_flag` returns a string. Mapping, conditional, coalescing, extreme,
baseline value, and aggregate expressions retain the selected or aggregated
value type.
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
  `dataset`, and returns column `value`. `source` and `key` are each one value or
  a list of values. A scalar means a one-element list, so an existing
  specification keeps its meaning. The lists pair by position: a record matches
  when `source[i]` equals `key[i]` for every position, and the two lists must
  have the same length. Pair order does not affect the result. The dictionary
  must be unique on the `key` columns taken together. A missing source returns
  `missing` when supplied; with several sources that condition holds when any
  one of them is missing, because a partial key cannot identify a record. A key
  whose sources are all non-missing and that matches no record returns
  `unmapped` when supplied. Otherwise either condition fails.
- `cut` assigns one of `len(breaks) + 1` labels using ascending breaks. Labels
  must have that exact length. With `right: false`, intervals are left-closed
  and right-open. Missing input returns `missing` when supplied, or fails.
- `str_extract` matches `pattern` as an ECMAScript regular expression and
  returns capture group `group`, where zero is the complete match. Missing input
  returns `missing` when supplied. A non-missing input with no match returns
  `no_match` when supplied. Otherwise either condition fails.
- `str_concat` concatenates all expressions in `sources` in order. A missing source returns `missing` when supplied. Otherwise it fails on missing input.
- `str_upper` converts all characters in `source` to uppercase. A missing source returns `missing` when supplied. Otherwise it fails on missing input.
- `str_lower` converts all characters in `source` to lowercase. A missing source returns `missing` when supplied. Otherwise it fails on missing input.

### Numeric, selection, and conditional expressions

- `compute` evaluates `expr` as a scalar numeric formula over current-output
  columns and numeric literals. R010 defines its grammar, its closed function
  vocabulary, type promotion, `NULL` propagation, and failure conditions. It is
  scalar: it must not contain an aggregate, a window function, a comparison, a
  conditional, a string, or a host-language call, so it cannot bypass the
  evaluation-kind rules above. It is the only arithmetic expression: it accepts
  a column in every operand position and needs no guarding predicate for
  missing inputs.
- `coalesce` returns the first non-missing variable in `sources`. If all are
  missing, it returns the literal `default` when supplied, or missing.
- `greatest` returns the largest non-missing variable in `sources` and `least`
  the smallest, or missing when every source is missing. They reduce across
  the columns of one row, which is what distinguishes them from `min` and
  `max`, and they place no restriction on type beyond comparability, which is
  what distinguishes them from R010's `GREATEST` and `LEAST`. Use the R010
  functions inside a numeric formula and these expressions to derive a column,
  including a column of dates.
- `case` evaluates branches in order and returns the `then` expression of the
  first `TRUE` predicate. It returns `else` when no branch matches, or missing
  when `else` is absent.

### Date expression

- `date_diff` returns the count of whole `unit` intervals from `start` to `end`,
  negative when `end` precedes `start`. It excludes the start point. Permitted
  units are `day`, `week`, `month`, and `year`.

### Window expressions

- `row_number` numbers rows from one within each partition, by the order terms
  in `order_by` under the ordering rule above. Its optional `filter` is a
  predicate over constructed output rows, evaluated before partitioning: a row
  for which it is not `TRUE` receives missing, and surviving rows are numbered
  from one within their partition as though the excluded rows had not been
  constructed. A partition in which no row survives yields no number rather
  than an error, so a rank of one always identifies a record that satisfied the
  filter. Identifiers in `filter` are dependencies under R001, exactly as in
  `case.branches[].when`. Encoding the same condition as a leading order term
  is not equivalent: ordering ranks an excluded row last but still numbers it,
  so rank one would not imply the condition.
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
- A `row_number` filter that is not a Boolean predicate over current-output
  columns: fail.
- An aggregate outside its two permitted contexts: fail.
- `mapping_from` whose `source` and `key` lists differ in length: fail.
- An unhandled local missing, mapping, or extraction condition: fail.
- A `compute` expression that violates R010: fail.
- An unresolved function, failed function call, or non-scalar function result:
  fail with the function name and original runtime context.
