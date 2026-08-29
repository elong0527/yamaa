---
id: R001
title: Execution Model
status: normative
applies_to: [root.base, root.rows, row.dataset, root.columns, derivation]
depends_on: [R002, R003, R004, R005, R007, R008, R010]
---

# Execution model

## Intent

Define how output rows, columns, and derivation expressions are evaluated
without relying on YAML declaration order for dependencies.

## Boundaries

This rule owns the two phases, dependency inference, and evaluation order. It
does not define what an expression means (R007), how a name binds to a source
(R002), or what happens to a result after its expression completes (R005).

## Phases

Derivation has two phases:

1. Row construction evaluates `rows` entries and may change row count.
2. Column derivation enriches constructed rows and must not change row count.

Each `rows` entry uses its explicit `dataset` as the row driver. If `dataset`
is omitted, it uses root `base`. `base` is optional when every row declares a
dataset. Constructed rows are appended in specification order.

When `rows` is absent or empty, row construction produces exactly one output
row per `base` record, in base-record order. `base` is required in that case.

## Expression evaluation

An expression contains exactly one keyword registered by R007. Most keywords
name their input variables directly. Resolve those variable dependencies, then
evaluate the keyword. Fields explicitly typed as `expression` are evaluated
recursively. A `source` or `literal` expression is a leaf. YAML mapping order
has no execution meaning.

Window expressions evaluate over the partitions declared by their own
`group_by`. Aggregate expressions evaluate in the two contexts R007 permits.
All other expressions return one value per current row.

## Dependency execution

Implementations must infer dependencies rather than evaluate columns in YAML
declaration order. Recursively traverse each expression and collect:

- every unqualified output variable referenced by `source`;
- variables in `group_by`, `order_by`, and other fields typed as `variable`;
- variables referenced by fields typed as nested `expression`;
- current-output identifiers used by an `sql` predicate;
- current-output identifiers used by a `numeric_expression`.

Predicates include `case.branches[].when`, `override[].when`, `row.filter`,
aggregate `filter`, and window `filter`. Identifier extraction requires parsing
the predicate under the R004 grammar and the numeric expression under the R010
grammar; an implementation must not treat either as dependency-free.

For each row definition, evaluate row derivations using a dependency graph.
Row derivations cannot depend on values produced only during the column phase.

After row construction, build the column dependency graph and evaluate it in
topological order. When several columns are ready, declaration order is the
deterministic tie-breaker.

In both phases, a completed derivation runs the R005 lifecycle before anything
depends on it, so a dependent always reads a value of the declared type.

Column declaration order controls final layout, not evaluation order.

## Errors

- A row without an explicit `dataset` or default `base`: fail.
- A specification with no `rows` entry and no `base`: fail.
- A row dependency on a later-phase value: fail.
- An unresolved variable or predicate reference: fail.
- A dependency cycle: fail and report the cycle path.
- An expression that changes row count during column derivation: fail.
