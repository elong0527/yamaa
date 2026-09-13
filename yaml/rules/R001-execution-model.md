---
id: R001
title: Execution Model
status: normative
applies_to: [root.base, root.rows, row.dataset, row.group_by, row.filter,
  root.columns, derivation]

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

**R001-1.** Derivation has two phases:

1. **R001-2.** Row construction evaluates `rows` entries and may change row
   count.
2. **R001-3.** Column derivation enriches constructed rows and must not
   change row count.

**R001-4.** Each `rows` entry uses its explicit `dataset` as the row driver.
If `dataset` is omitted, it uses root `base`. `base` is optional when every
row declares a dataset, and when `datasets` declares exactly one dataset:
that dataset is the default driver.

**R001-5.** A row template has one of two modes:

1. **R001-6.** A template without `group_by` is record-driven. Its `filter`,
   when present, evaluates against each driver record before any row
   derivation. Every retained driver record produces one candidate row.
2. **R001-7.** A template with `group_by` is group-driven. Its non-empty list
   names only qualified variables of its row driver. The complete driver
   relation is partitioned by the equality each value's type owns, including
   R019 for strings, with missing values equal to other missing values for
   grouping. Every group produces one candidate row.

**R001-8.** Groups are ordered by the position of their first driver record.
Within a group, records retain driver order. For each group, evaluate every
row derivation once and complete stages 1 through 4 of the R005 lifecycle.
Then evaluate the template's `filter`, when present, over the candidate's
completed unqualified columns. Append the candidate only when the predicate
is `TRUE`; `FALSE` or `UNKNOWN` suppresses it. A grouped `filter` therefore
corresponds to filtering after a group reduction, while an ungrouped
`filter` retains its existing driver-record meaning.

**R001-9.** Constructed rows are appended in specification order, using driver
order for record-driven templates and first-occurrence group order for
group-driven templates.

**R001-10.** The output row grain belongs to the input relations and the row
templates written in the specification. Row construction has no operation
that repeats a candidate a data-dependent number of times and no generated
index for such a repetition. A source value may decide whether a written
template retains its one candidate, but it cannot create additional
instances of that template.

**R001-11.** When the required artifact has one row per observation,
administration, or planned event, an input relation must therefore contain
one driver record per required row. Expected-but-uncollected rows use an
explicit planning relation at that grain and may be enriched from collected
relations through record lookups. Dynamically counted expansion must happen
upstream and its expanded records enter the specification as ordinary input.

**R001-12.** Row construction first derives the distinct combination of `keys`
over the driver records, in first-appearance order. That key table is
standalone: one row per unique key combination, with no link back to the
driver records. When `rows` is absent or empty, the key table is the output
row set. When `rows` is present, each row template is one section: its filter
selects feeding records, each surviving record yields its rows for that key
combination, and the sections concatenate. `base` is required when `rows` is
absent or empty, unless `datasets` declares exactly one dataset, which
drives. `keys` must be declared.

**R001-12a.** A column derivation must yield exactly one value per row: zero
values is missing, more than one value for one key combination is a failure
under R001-44. In a specification without `rows`, a key column derivation
must not depend on a non-key output column (R001-43); keys are derived before
any row logic runs.

## Expression evaluation

**R001-13.** An expression contains exactly one keyword registered by R007.
Most keywords name their input variables directly. Resolve those variable
dependencies, then evaluate the keyword. Fields whose declared type contains
`expression` are evaluated recursively. A `source` or `literal` expression is
a leaf. YAML mapping order has no execution meaning.

**R001-14.** Window expressions evaluate over the partitions declared by their
own `group_by`. Aggregate expressions evaluate in the contexts R007 permits.
All other expressions return one value per current row.

**R001-15.** During group-driven row construction, a source field of the row
driver is a scalar only when that exact qualified variable occurs in the
template's `group_by`. An aggregate expression may instead reduce the records
of the current driver group under R007 and R013. Other row expressions
consume group keys, literals, earlier row-derived columns, or a record lookup
whose matching values are already complete, in the ordinary dependency order.

## Dependency execution

**R001-16.** Implementations must infer dependencies to validate declaration
order and detect cycles. Recursively traverse each expression and collect:

- **R001-17.** every unqualified output variable referenced by `source`;
- **R001-18.** the `source` and `between.value` variables of a record lookup
  any qualified reference names, or its applicable output keys when those
  fields are omitted, which R015 defines;
- **R001-19.** variables in `group_by`, `order_by`, and other fields typed as
  `variable`;
- **R001-20.** variables passed as leaves in `function.args`;
- **R001-21.** variables referenced by fields whose type contains nested
  `expression`;
- **R001-22.** current-output identifiers used by an `sql` predicate;
- **R001-23.** current-output identifiers used by a `numeric_expression`;
- **R001-24.** identifiers used by an `aggregate_expression`;
- **R001-25.** variables used as placeholders in a `string_template`.

**R001-26.** Predicates include `case.branches[].when`, `override[].when`,
`row.filter`, aggregate `filter`, and window `filter`. An ungrouped
`row.filter` resolves only driver variables and runs before its derivation
graph. A grouped `row.filter` resolves unqualified columns derived by that
row template and runs after the whole graph completes. Identifier extraction
requires parsing the predicate under the R004 grammar, the numeric expression
under the R010 grammar, the string template under the R012 grammar, and the
reducer expression under the R013 grammar; an implementation must not treat
any of them as dependency-free.

**R001-27.** For each row definition, evaluate row derivations using a
dependency graph. Row derivations cannot depend on values produced only
during the column phase. Every unqualified identifier in a grouped
`row.filter` must resolve to a column derived by that same template. The
filter itself is not a derivation and adds no graph edge between columns
because it runs only after all of them have completed.

**R001-28.** After row construction, build the column dependency graph. Every
dependency must refer to a column declared earlier. Evaluate columns in
declaration order. When a specification declares `parents`, R017 composes,
prunes, and orders the resolved columns before this rule applies; declaration
order here is that resolved order.

**R001-29.** The graph is over columns, not over rows. A column that reads
another row of its own partition therefore depends on the whole column it
names, so a column that reaches its own value that way is a cycle rather
than an iteration. `previous_non_missing` crosses any number of missing rows
by searching a separate completed source column; conventional carry-forward
coalesces the current source with that result. Searching the column being
derived remains a cycle rather than an instruction to iterate.

**R001-30.** In both phases, a completed derivation runs the R005 lifecycle
before anything depends on it, so a dependent always reads a value of the
declared type.

**R001-31.** `output.columns` selects and orders artifact columns
independently of declaration order, and `output.order_by` orders the
artifact's rows independently of the construction order this rule defines.
R005 owns both.

## Rationale

Row count changes only during row construction, so a reviewer can tell row
grain from enrichment: the templates and their driver relations fix how many
rows exist before any column is derived. Dependency inference keeps
declaration order checkable and makes cycles visible instead of leaving
evaluation order to mapping order or to repeated reads of one partition.

## Errors

- **R001-32.** A row without an explicit `dataset` or default driver: fail.
- **R001-33.** A specification with no `rows` entry and no default driver:
  fail. The default driver is root `base`, or the single declared dataset
  when `base` is omitted.
- **R001-34.** An empty or duplicate `row.group_by`: fail.
- **R001-35.** A `row.group_by` variable not qualified to that row's driver:
  fail.
- **R001-36.** A grouped row derivation reading a non-grouped driver field
  without an aggregate: fail and report the field.
- **R001-37.** An ungrouped `row.filter` naming an output column, or a grouped
  `row.filter` naming a qualified variable or a column not derived by that
  template: fail.
- **R001-38.** A row dependency on a later-phase value: fail.
- **R001-39.** An unresolved variable or predicate reference: fail.
- **R001-40.** A reference to a later declared column: fail and report both
  columns.
- **R001-41.** A dependency cycle: fail and report the cycle path.
- **R001-42.** An expression that changes row count during column derivation:
  fail.
- **R001-43.** In a specification without `rows`, a key column derivation
  depending on a non-key output column: fail. Keys are derived before any row
  logic runs.
- **R001-44.** A column derivation yielding more than one value for one key
  combination: fail and report the column and the keys.
