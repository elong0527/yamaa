---
id: R001
title: Execution Model
status: normative
applies_to: [root.base, root.rows, row.dataset, root.columns]
depends_on: [R002, R003, R005, R007, R008]
---

# Execution model

## Intent

Define how output rows and columns are constructed without relying on YAML
declaration order for derivation dependencies.

## Phases

Derivation has two phases:

1. Row construction evaluates `rows` entries and may change row count.
2. Column derivation enriches constructed rows and must not change row count.

Each `rows` entry uses its explicit `dataset` as the row driver. If `dataset`
is omitted, it uses root `base`. `base` is optional when every row declares a
dataset. Constructed rows are appended in specification order.

When `rows` is absent or empty, row construction produces exactly one output row
per `base` record, in base-record order. `base` is required in that case. This
is the ordinary shape for a one-record-per-subject dataset, where every output
column has a column-level derivation and no row template is needed.

## Dependency execution

Implementations must infer dependencies rather than evaluate columns in YAML
declaration order.

Dependencies include current-output variable references in `source`, every
`{source: VARIABLE}` expression nested in `operations`, `group_by`, and
ordering arguments. Lookup keys required by R003 are also dependencies.

Dependencies also include every current-output variable named inside a
predicate. A predicate is any argument whose registry signature declares the
type `sql` under R007 or R008, such as `case.when` and `override.rules[].when`.
Implementations must extract identifier references from the predicate and add
them to the dependency graph.

Predicate references are easy to miss because they are ordinary strings rather
than `{source: VARIABLE}` expressions. Omitting them does not raise an error; it
silently evaluates a derivation before its input exists. Identifier extraction
depends on the filter grammar, which R004 leaves unresolved, so implementations
must not treat a predicate as dependency-free while that grammar is open.

For each row definition, evaluate row derivations using a dependency graph.
Row derivations cannot depend on values produced only during the later column
phase.

After row construction, build the column dependency graph and evaluate it in
topological order. Operations within one derivation are evaluated in listed
order. When multiple columns are ready, declaration order is the deterministic
tie-breaker. Convert each result according to R005 before making it available
to dependents.

Column declaration order controls final column layout, not evaluation order.

## Errors

- A row without an explicit `dataset` or default `base`: fail.
- A specification with no `rows` entry and no `base`: fail.
- A variable named in a predicate that is absent from the dependency graph:
  fail.
- A row dependency on a later-phase value: fail.
- An unresolved variable reference: fail.
- A dependency cycle: fail and report the cycle path.
- A column derivation that changes row count: fail.
