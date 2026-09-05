# YAML derivation specification

This folder defines a compact, language-agnostic specification for ODM-to-SDTM
and SDTM-to-ADaM derivations. The design is under active development.

## Contents

- `schema.yaml` is the schema-bundle entry point and defines shared structure.
- `schema_environment.yaml` is the separately validated project-environment
  entry point.
- `schema_derivation.yaml`, `schema_expression_*.yaml`, and
  `schema_verification.yaml` register and document closed derivation and
  verification types.
- `schema_function.yaml` registers calls to functions resolved by R018's
  project environment.
- `rules/` contains shared execution semantics, with one rule per file.
- `examples/` contains source data, derivation specifications, and exact
  expected outputs.
- `agents.md` tells AI coding agents how to discover and maintain the design.

The schema defines shape and operation-local behavior through adjacent comments
and validation-neutral parameter descriptions. Rule files define behavior
shared across operations. Examples demonstrate both without redefining them.

R019 gives every string one portable contract: language source is ASCII, data
may contain Unicode scalar values, casing and case-insensitive mapping affect
ASCII letters only, no normalization is implicit, and equality and ordering
operate on the exact scalar sequence.

## Version 1.0 design boundary

Operations consume named variables rather than arbitrary nested expressions.
This keeps every operation self-contained, exposes dependencies, and avoids
mixed argument shapes. Multi-step derivations use named columns as intermediate
values. Nested expressions remain only where nesting is intrinsic: `case`
results, string concatenation inputs, and final `override` values.

This is the version 1.0 direction for team review.

Three closed mini-languages are narrow exceptions to fields that name
their inputs directly. Registering an operator per arithmetic operation makes a
single formula such as
`WEIGHTKG / POWER(HEIGHTCM / 100, 2)` into several columns and grows the
registry without end, so `compute` takes one closed numeric expression instead
and is the only arithmetic expression. It stays inside the boundary's purpose:
its payload is a leaf field, not a nested argument tree, and R001 extracts its
identifiers exactly as it already extracts them from `case.branches[].when`, so
dependencies remain visible. R010 closes its grammar and function vocabulary
and confines it to numeric results, so it cannot displace the typed string,
date, mapping, or conditional expressions.

`str_template` is the string counterpart. It permits literal text and braced
variable placeholders only. R001 extracts every placeholder as a dependency,
and R012 fixes its grammar and escaping, so it cannot become host-language
evaluation or displace typed string operations. `str_concat` remains the form
that composes nested expressions.

`aggregate` is the third, and it replaced `min`, `max`, `sum`, and `count` for
the reason `compute` replaced the arithmetic operators: an entry per reducer
grows the registry without end and cannot express arithmetic over the records
being reduced. R013 closes its reducer table and its grammar, requires every
identifier to name one relation, and requires every identifier to sit inside a
reduction unless the reduction groups on it. Those three limits keep it from
becoming a join, a window, or a second spelling of `compute`, and they leave
where an aggregate may be used with R007 and the join that consumes it with
R003. A qualified aggregate may additionally narrow that relation against one
current-row value through inclusive lower or upper bounds; R013 fixes the
one-sided and missing-value behavior.

A reduction at one grain followed by a reduction at another remains two
specifications. The first artifact is a normal stored source of the second, so
each grain keeps its own identity and validation contract without adding
in-memory datasets or inferring pipeline order from paths.

A row template may declare `group_by` when its driver records must first form
one candidate row per group. Aggregate row derivations reduce the current
driver group, scalar derivations compose those results, and the template's
existing `filter` then decides whether the completed candidate is appended.
Without `group_by`, `row.filter` keeps its original meaning of selecting driver
records before derivation. R001 owns both modes, and R013's `ONLY` reducer makes
a required single source record reject duplicates rather than choose one.

`output.columns` keeps binding columns out of the final dataset while retaining
them as named intermediate values. A `compute` formula may also read a numeric
field directly from a declared record lookup; this avoids a binding column when
the lookup already gives the selected record a stable name.

`output.order_by` is the row counterpart. It declares the order the artifact
presents its rows in, reusing the order terms every window expression and
record lookup already declares, and leaves R001's construction order to decide
how rows are built. A term may name an intermediate column, so a submission
order can rest on a numeric ordinal the artifact itself does not carry. R005
defines it.

A `record_lookups` entry names one record of another dataset so that several
columns can read it, which no expression can do while each returns one value.
It is not an expression and adds no nesting: its matching, filtering, and
ordering fields are the ones `mapping_from` and `multiple_matches` already
declare, and a column reads it through the qualified variable form it already
uses for a dataset. R015 defines it.

The version 1.0 input-shape audit covers every registered expression:

| Expressions | Input policy |
|---|---|
| `source`, `literal` | Leaf expressions; unchanged |
| `mapping`, `cut`, `str_extract`, `str_upper`, `str_lower` | One named source; exceptional results are literals |
| `str_concat` | An ordered list of expressions, because concatenating requires literals beside sources |
| `str_template` | One closed string template over named variables (R012) |
| `mapping_from` | One or more named sources paired by position with declared right-side key columns; exceptional results are literals |
| `compute` | One closed numeric expression over named output columns and declared record-lookup fields (R010) |
| `date_diff`, `study_day` | Named variable operands; `date_diff` declares which endpoints it counts |
| `date_impute` | One named source, an integer literal or a month-relative token for each imputed component, an optional minimum collected precision, and an optional named lower bound on the completed date; exceptional results are literals |
| `date_precision` | One named source, either collected text or a temporal value; exceptional results are literals |
| `to_date` | One named `datetime` source; no literals or nesting |
| `coalesce` | Ordered named variables plus an optional literal default |
| `greatest`, `least` | Named variables reduced across one row; no literals and no nesting |
| `row_number`, `rank`, `baseline_flag`, `baseline_value` | Named grouping, ordering, and value variables |
| `row_value` | One named source with named grouping and ordering variables, plus a signed integer literal offset along the declared order |
| `previous_non_missing` | One named source searched strictly backward through named grouping and ordering variables |
| `aggregate` | One closed reducer expression over one relation, optionally narrowed by a current-row range (R013) |
| `case` | Nested result expressions retained because selecting expressions is its purpose |
| `function` | Closed named arguments are variables or scalar literal leaves; string and temporal literals use explicit tagged forms (R018) |

At the derivation-result level, `conversion_failure` is a literal and
`override.value` remains an expression because a final correction may select a
source, literal, or another registered operation.

`function` is the deliberate extensibility boundary. A portable specification
may declare a logical call before project code is implemented. When an actual
implementation is supplied, the runner selects one project root containing one
`environment.yaml`; the specification cannot override it. R018 closes its
versioned contracts, singular R or Python runtime, bindings, and conformance
vectors at that implementation stage.

## Review workflow

1. Review the root field in `schema.yaml` and its included schema module.
2. Review every applicable rule listed in `rules/README.md`.
3. Review at least one positive example and its expected output.
4. Add a negative example when the rule defines an error condition.
5. Require R and Python implementations to produce equivalent outputs and
   errors from the same examples.

Behavior not defined by a normative rule must not be inferred by an
implementation. It should be proposed as a new rule or marked as an unresolved
design question.
