# YAML derivation specification

This folder defines a compact, language-agnostic specification for ODM-to-SDTM
and SDTM-to-ADaM derivations. The design is under active development.

## Contents

- `schema.yaml` is the schema-bundle entry point and defines shared structure.
- `schema_derivation.yaml`, `schema_expression_*.yaml`, and
  `schema_verification.yaml` register closed derivation and verification types.
- `schema_function.yaml` registers calls to functions resolved by the project's
  global execution environment.
- `rules/` contains the execution semantics, with one rule per file.
- `examples/` contains source data, derivation specifications, and exact
  expected outputs.
- `agents.md` tells AI coding agents how to discover and maintain the design.

The rule files are the authoritative source for behavior. The schema defines
shape, while examples demonstrate rules without redefining them.

## Version 1.0 design boundary

Operations consume named variables rather than arbitrary nested expressions.
This keeps every operation self-contained, exposes dependencies, and avoids
mixed argument shapes. Multi-step derivations use named columns as intermediate
values. Nested expressions remain only where nesting is intrinsic: `case`
branch results, runtime-function arguments, and final `override` values.

This is the version 1.0 direction for team review.

`compute` is the one exception, and it is a narrow one. Registering an operator
per arithmetic operation makes a single formula such as
`WEIGHTKG / POWER(HEIGHTCM / 100, 2)` into several columns and grows the
registry without end, so `compute` takes one closed numeric expression instead
and is the only arithmetic expression: `multiply`, `add`, `subtract`, and
`percent_change` were deleted when it landed.
It stays inside the boundary's purpose: its payload is a leaf field, not a
nested argument tree, and R001 extracts its identifiers exactly as it already
extracts them from `case.when`, so dependencies remain visible. R010 closes its
grammar and function vocabulary and confines it to numeric results, so it
cannot displace the typed string, date, mapping, or conditional expressions.
`column.output` keeps any binding column it needs out of the final dataset.

The version 1.0 input-shape audit covers every registered expression:

| Expressions | Input policy |
|---|---|
| `source`, `literal` | Leaf expressions; unchanged |
| `mapping`, `cut`, `str_extract` | One named source; exceptional results are literals |
| `mapping_from` | One or more named sources paired by position with declared right-side key columns; exceptional results are literals |
| `compute` | One closed numeric expression over named output columns (R010) |
| `date_diff` | Named variable operands |
| `coalesce` | Ordered named variables plus an optional literal default |
| `row_number`, `baseline_flag`, `baseline_value` | Named grouping, ordering, and value variables |
| `min`, `max` | One named variable or structured source binding |
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
3. Review at least one positive fixture and its expected output.
4. Add a negative fixture when the rule defines an error condition.
5. Require R and Python implementations to produce equivalent outputs and
   errors from the same fixtures.

Behavior not defined by a normative rule must not be inferred by an
implementation. It should be proposed as a new rule or marked as an unresolved
design question.
