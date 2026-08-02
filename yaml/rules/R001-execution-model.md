---
id: R001
title: Execution Model
status: normative
applies_to: [root.base, root.rows, row.dataset, root.columns]
depends_on: [R002, R003, R005]
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

## Dependency execution

Implementations must infer dependencies rather than evaluate columns in YAML
declaration order.

Dependencies include current-output variable references in `source`, function
arguments, `group_by`, ordering arguments, and derivation filters. Lookup keys
required by R003 are also dependencies.

For each row definition, evaluate row derivations using a dependency graph.
Row derivations cannot depend on values produced only during the later column
phase.

After row construction, build the column dependency graph and evaluate it in
topological order. When multiple nodes are ready, declaration order is the
deterministic tie-breaker. Convert each result according to R005 before making
it available to dependents.

Column declaration order controls final column layout, not evaluation order.

## Errors

- A row without an explicit `dataset` or default `base`: fail.
- A row dependency on a later-phase value: fail.
- An unresolved variable reference: fail.
- A dependency cycle: fail and report the cycle path.
- A column derivation that changes row count: fail.
