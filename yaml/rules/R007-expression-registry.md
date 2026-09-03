---
id: R007
title: Expression Registry
status: normative
applies_to: [expression, expressions, schema_expression]
depends_on: [R001, R002, R003, R004, R005, R006, R008, R010, R011, R012, R013, R014, R015, R016, R018]
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
R013 for aggregate reduction, R014 for the type a source field carries, R015
for a record selected once and read by several columns, R016 for dates and
datetimes, R018 for project functions, and R004 for predicates.

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
| `override[].value` | A final correction may select any expression |

`derivation` and `handled_expression_class.value` also contain `expression`,
but they hold a derivation's own top-level expression rather than nest one
inside an operation, so this policy does not restrict them.

Fields typed `numeric_expression`, `string_template`, and
`aggregate_expression` are leaves whose identifiers R010, R012, and R013
resolve. Plain strings are values unless their schema field is typed as
`variable`, `function_arg`, `sql`, or `string_template`. R018 closes
`function_arg`: a string is a variable, while string, date, and datetime
literals use their explicit tagged leaf forms.

## Evaluation kinds

Scalar expressions return one value per row. Window expressions partition
constructed output rows by their local `group_by` and preserve row count.
Omitting `group_by` creates one partition. A window that declares `filter`
still preserves row count: an excluded row receives missing rather than being
dropped. A window that reads another row of its partition returns missing when
that row does not exist, which is the same result as a neighbouring row whose
value is missing.

`aggregate` is the only aggregate expression. R013 defines its grammar, the
reducers it permits, and what each returns; this rule fixes where it may be
used. It is valid in exactly three contexts:

1. Its identifiers are qualified to one declared dataset relation during
   column derivation. It then reduces that right side before the R003 join,
   which R003 defines. The qualifier may equal the current row driver because
   an aggregate reads the relation rather than the scalar driver record.
2. Its identifiers are unqualified. It then declares `group_by`, reduces
   constructed output rows within each partition, and broadcasts the result to
   each row.
3. It is a row derivation of a grouped row template and every identifier is
   qualified to that template's row driver. It reduces the records of the
   current driver group to one candidate-row value. The enclosing
   `row.group_by` owns the grain, so the aggregate itself omits `group_by`.

Any other aggregate context is an error. A `filter` narrows the records the
owning expression already works in: right-side records for context 1, and
constructed output rows for a window or for context 2, and current driver-group
records for context 3. `between` is valid only in context 1 and narrows those
right-side records separately for each current row under R013.

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

Non-missing values use the order their type owns: numeric order under R010,
code-point sequence order for `str` under R004, and chronological order for
`date` and `datetime` under R016. An implementation must not substitute host
locale collation for the R004 order.

That tie-break settles positions, not equality. `row_number`, `row_value`, and
right-side selection read the positions themselves, so a tie changes which row
they reach. `rank` compares only the declared terms, so records equal on every
one of them receive a single number rather than the distinct numbers their
positions would give. Its `competition` method leaves the positions occupied by
a tie out of the subsequent numbers; its `dense` method numbers distinct values
consecutively. A specification that wants a tie broken declares the term that
breaks it, whichever method it uses.

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
- `date_diff`, `study_day`, `date_impute`, and `date_precision` state their
  own input types in R016;
- `greatest` and `least` require mutually comparable `sources`;
- `row_value` requires an integer `offset` and accepts any `source` type;
- window ordering requires mutually comparable values. One order term names one
  variable, and a variable has exactly one type -- R014
  gives it to a source field and R011 to a declared column -- so the values a
  term compares are of one type by construction and ordering has no
  incomparable case. An expression naming several variables, as `greatest` and
  `least` do, is where comparability is a requirement rather than a
  consequence;
- `aggregate` states its own input types in R013.
- `function` states its exact argument and result types in R018.

**Comparability is a property of the runtime type.** `int` and `float` are
mutually comparable, because R010 promotes them. Every other type is
comparable only with itself. Collected precision, which R016 defines, is not a
runtime type and so takes no part in comparability: two temporal values of one
type are comparable whatever precision each carries. A comparable type
therefore satisfies any input requiring mutually comparable values --
`greatest` and `least`, `mapping_from` key pairing, an `order_by` term, and
R013's `MIN` and `MAX` -- while a `sources` list or one ordering term mixing
two types is the incompatible-input error below rather than a comparison over
a coerced operand. Each owning rule defines the order its type takes.

`source` retains its source type, which R014 defines, and `literal` retains
its YAML scalar type after R011's non-finite normalization.
`cut`, `str_extract`, `str_concat`, `str_template`, `str_upper`, and
`str_lower` return strings. `compute` returns the numeric type its expression
promotes to under R010.
`row_number` and `rank` return integers. The temporal operations return the
types R016 gives them. `baseline_flag` returns a string. Mapping, conditional,
coalescing, extreme, baseline value, and offset row expressions retain the
selected value type, and a selected temporal value carries its collected
precision unchanged.
`aggregate` returns the type R013 gives its expression. R018 gives `function`
the return type declared by its logical contract.

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
- A `sources` list or an ordering term mixing two runtime types that are not
  mutually comparable: fail rather than convert an operand.
- A scalar or window expression that changes row count: fail under R001, which
  owns the phase invariant.
- A window expression used during row construction: fail.
- A window `filter` that is not a Boolean predicate over current-output
  columns: fail.
- A `row_value` whose `offset` is zero: fail. The current row's own value is
  `source`, and a window must not be a second spelling of it.
- An aggregate outside its three permitted contexts: fail.
- An aggregate declaring `between` outside the qualified dataset context:
  fail.
- A grouped-row aggregate naming a dataset other than its row driver or
  declaring its own `group_by`: fail.
- An `aggregate` expression that violates R013: fail.
- `mapping_from` whose `source` and `key` lists differ in length: fail.
- An unhandled local missing, mapping, or extraction condition: fail under
  R008.
- A `compute` expression that violates R010: fail.
- A `str_template` expression that violates R012: fail.
- A temporal value or operation that violates R016: fail.
- A project function that violates its environment, contract, binding, or
  result requirements: fail under R018.
