# YAML derivation specification

This folder defines a compact, language-agnostic specification for ODM-to-SDTM
and SDTM-to-ADaM derivations. The design is under active development.

## Contents

- `schema.yaml` is the schema-bundle entry point and defines shared structure.
- `schema_derivation.yaml`, `schema_expression_*.yaml`, and
  `schema_verification.yaml` register and document closed derivation and
  verification types.
- `schema_function.yaml` registers calls to functions resolved by the project's
  global execution environment.
- `rules/` contains shared execution semantics, with one rule per file.
- `examples/` contains source data, derivation specifications, and exact
  expected outputs.
- `agents.md` tells AI coding agents how to discover and maintain the design.

The schema defines shape and operation-local behavior through adjacent comments
and validation-neutral parameter descriptions. Rule files define behavior
shared across operations. Examples demonstrate both without redefining them.

## Version 1.0 design boundary

Operations consume named variables rather than arbitrary nested expressions.
This keeps every operation self-contained, exposes dependencies, and avoids
mixed argument shapes. Multi-step derivations use named columns as intermediate
values. Nested expressions remain only where nesting is intrinsic: `case`
results, string concatenation inputs, runtime-function arguments, and final
`override` values.

This is the version 1.0 direction for team review.

Two closed scalar mini-languages are narrow exceptions to fields that name
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

`column.output` keeps any binding column it needs out of the final dataset.

The version 1.0 input-shape audit covers every registered expression:

| Expressions | Input policy |
|---|---|
| `source`, `literal` | Leaf expressions; unchanged |
| `mapping`, `cut`, `str_extract`, `str_upper`, `str_lower` | One named source; exceptional results are literals |
| `str_concat` | An ordered list of expressions, because concatenating requires literals beside sources |
| `str_template` | One closed string template over named variables (R012) |
| `mapping_from` | One or more named sources paired by position with declared right-side key columns; exceptional results are literals |
| `compute` | One closed numeric expression over named output columns (R010) |
| `date_diff`, `study_day` | Named variable operands; `date_diff` declares which endpoints it counts |
| `date_impute` | One named source plus integer literals for the imputed components; exceptional results are literals |
| `coalesce` | Ordered named variables plus an optional literal default |
| `greatest`, `least` | Named variables reduced across one row; no literals and no nesting |
| `row_number`, `baseline_flag`, `baseline_value` | Named grouping, ordering, and value variables |
| `row_value` | One named source with named grouping and ordering variables, plus a signed integer literal offset along the declared order |
| `min`, `max`, `sum`, `count` | One named variable or structured source binding |
| `case` | Nested result expressions retained because selecting expressions is its purpose |
| `function` | Named arguments may be variables, non-string literals, or explicit expressions |

At the derivation-result level, `conversion_failure` is a literal and
`override.value` remains an expression because a final correction may select a
source, literal, or another registered operation.

`function` is the deliberate extensibility boundary. Its environment, available
functions, and package setup are global project configuration rather than fields
repeated at each call site.

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
