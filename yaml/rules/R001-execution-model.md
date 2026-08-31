---
id: R001
title: Execution Model
status: normative
applies_to: [root.base, root.rows, root.group_by, root.expand, row.dataset,
  root.columns, derivation]
depends_on: [R002, R003, R004, R005, R007, R008, R010, R012, R013, R015]
---

# Execution model

## Intent

Define how output rows, columns, and derivation expressions are evaluated in
an explicit, reviewable dependency order.

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

When `rows` is absent or empty and neither `group_by` nor `expand` is declared,
row construction produces exactly one output row per `base` record, in
base-record order. `base` is required in that case.

An artifact may replace ordinary `rows` construction with exactly one of two
other row-construction modes:

- `group_by` requires `base` and constructs one row for each distinct tuple of
  its base variables. Tuples appear in the order their first base record
  appears. The variables must be qualified fields of that base. They are the
  only base fields a scalar expression may read directly on the grouped row;
  an aggregate may reduce the base records in the group normally.
- `expand` requires `base` and constructs `count` rows for each base record, in
  base-record order. Within each record, `as` receives the integers from 1
  through `count` in order. `count` must resolve on the base record to a
  non-missing, non-negative integer. Zero contributes no row. R005 treats `as`
  as the row-phase derivation of that declared integer column.

`rows`, `group_by`, and `expand` are mutually exclusive. When all three are
absent, the ordinary one-row-per-`base` construction applies.

## Expression evaluation

An expression contains exactly one keyword registered by R007. Most keywords
name their input variables directly. Resolve those variable dependencies, then
evaluate the keyword. Fields whose declared type contains `expression` are
evaluated recursively. A `source` or `literal` expression is a leaf. YAML
mapping order has no execution meaning.

Window expressions evaluate over the partitions declared by their own
`group_by`. Aggregate expressions evaluate in the two contexts R007 permits.
All other expressions return one value per current row.

## Dependency execution

Implementations must infer dependencies to validate declaration order and
detect cycles. Recursively traverse each expression and collect:

- every unqualified output variable referenced by `source`;
- the `source` and `between.value` variables of a record lookup a qualified
  variable names, which R015 defines;
- variables in `group_by`, `order_by`, and other fields typed as `variable`;
- variables referenced by fields whose type contains nested `expression`;
- current-output identifiers used by an `sql` predicate;
- current-output identifiers used by a `numeric_expression`;
- identifiers used by an `aggregate_expression`;
- variables used as placeholders in a `string_template`.

Predicates include `case.branches[].when`, `override[].when`, `row.filter`,
aggregate `filter`, and window `filter`. Identifier extraction requires parsing
the predicate under the R004 grammar, the numeric expression under the R010
grammar, the string template under the R012 grammar, and the reducer
expression under the R013 grammar; an implementation must not treat any of
them as dependency-free.

For each row definition, evaluate row derivations using a dependency graph.
Row derivations cannot depend on values produced only during the column phase.

After row construction, build the column dependency graph. Every dependency
must refer to a column declared earlier. Evaluate columns in declaration order.

The graph is over columns, not over rows. A column that reads another row of
its own partition therefore depends on the whole column it names, so a column
that reaches its own value that way is a cycle rather than an iteration. A
value carried forward from a row that was itself carried forward is outside
this model.

In both phases, a completed derivation runs the R005 lifecycle before anything
depends on it, so a dependent always reads a value of the declared type.

`output.columns` selects and orders artifact columns independently of
declaration order.

## Errors

- A row without an explicit `dataset` or default `base`: fail.
- A specification with no row-construction declaration and no `base`: fail.
- More than one of `rows`, `group_by`, and `expand`: fail.
- An empty `group_by`, or a `group_by` variable that is unqualified, belongs to
  a dataset other than `base`, is repeated, or does not exist: fail.
- A scalar expression reading a non-grouped field of the grouped base: fail.
- An `expand.count` that is missing, non-integer, or negative: fail during row
  construction and report the base record.
- An `expand.as` that is undeclared, is not `int`, or has another derivation:
  fail.
- A row dependency on a later-phase value: fail.
- An unresolved variable or predicate reference: fail.
- A reference to a later declared column: fail and report both columns.
- A dependency cycle: fail and report the cycle path.
- An expression that changes row count during column derivation: fail.
