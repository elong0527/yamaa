---
id: R001
title: Execution Model
status: normative
applies_to: [root.base, root.rows, row.dataset, row.group_by, row.filter, root.columns, derivation]
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
dataset. A row template has one of two modes:

1. A template without `group_by` is record-driven. Its `filter`, when present,
   evaluates against each driver record before any row derivation. Every
   retained driver record produces one candidate row.
2. A template with `group_by` is group-driven. Its non-empty list names only
   qualified variables of its row driver. The complete driver relation is
   partitioned by equality on those values, with missing values equal to other
   missing values for grouping. Every group produces one candidate row.

Groups are ordered by the position of their first driver record. Within a
group, records retain driver order. For each group, evaluate every row
derivation once and complete stages 1 through 4 of the R005 lifecycle. Then
evaluate the template's `filter`, when present, over the candidate's completed
unqualified columns. Append the candidate only when the predicate is `TRUE`;
`FALSE` or `UNKNOWN` suppresses it. A grouped `filter` therefore corresponds
to filtering after a group reduction, while an ungrouped `filter` retains its
existing driver-record meaning.

Constructed rows are appended in specification order, using driver order for
record-driven templates and first-occurrence group order for group-driven
templates.

When `rows` is absent or empty, row construction produces exactly one output
row per `base` record, in base-record order. `base` is required in that case.

## Expression evaluation

An expression contains exactly one keyword registered by R007. Most keywords
name their input variables directly. Resolve those variable dependencies, then
evaluate the keyword. Fields whose declared type contains `expression` are
evaluated recursively. A `source` or `literal` expression is a leaf. YAML
mapping order has no execution meaning.

Window expressions evaluate over the partitions declared by their own
`group_by`. Aggregate expressions evaluate in the contexts R007 permits. All
other expressions return one value per current row.

During group-driven row construction, a source field of the row driver is a
scalar only when that exact qualified variable occurs in the template's
`group_by`. An aggregate expression may instead reduce the records of the
current driver group under R007 and R013. Other row expressions consume group
keys, literals, or earlier row-derived columns in the ordinary dependency
order.

## Dependency execution

Implementations must infer dependencies to validate declaration order and
detect cycles. Recursively traverse each expression and collect:

- every unqualified output variable referenced by `source`;
- the `source` variables of a record lookup any qualified reference names,
  which R015 defines;
- variables in `group_by`, `order_by`, and other fields typed as `variable`;
- variables referenced by fields whose type contains nested `expression`;
- current-output identifiers used by an `sql` predicate;
- current-output identifiers used by a `numeric_expression`;
- identifiers used by an `aggregate_expression`;
- variables used as placeholders in a `string_template`.

Predicates include `case.branches[].when`, `override[].when`, `row.filter`,
aggregate `filter`, and window `filter`. An ungrouped `row.filter` resolves only
driver variables and runs before its derivation graph. A grouped `row.filter`
resolves unqualified columns derived by that row template and runs after the
whole graph completes. Identifier extraction requires parsing the predicate
under the R004 grammar, the numeric expression under the R010 grammar, the
string template under the R012 grammar, and the reducer expression under the
R013 grammar; an implementation must not treat any of them as dependency-free.

For each row definition, evaluate row derivations using a dependency graph.
Row derivations cannot depend on values produced only during the column phase.
Every unqualified identifier in a grouped `row.filter` must resolve to a
column derived by that same template. The filter itself is not a derivation and
adds no graph edge between columns because it runs only after all of them have
completed.

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
- A specification with no `rows` entry and no `base`: fail.
- An empty or duplicate `row.group_by`: fail.
- A `row.group_by` variable not qualified to that row's driver: fail.
- A grouped row derivation reading a non-grouped driver field without an
  aggregate: fail and report the field.
- An ungrouped `row.filter` naming an output column, or a grouped `row.filter`
  naming a qualified variable or a column not derived by that template: fail.
- A row dependency on a later-phase value: fail.
- An unresolved variable or predicate reference: fail.
- A reference to a later declared column: fail and report both columns.
- A dependency cycle: fail and report the cycle path.
- An expression that changes row count during column derivation: fail.
