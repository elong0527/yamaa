---
id: R001
title: Execution Model
status: normative
applies_to: [root.base, root.rows, row.dataset, row.group_by, row.filter,
  root.columns, derivation]

---

# Execution model

![lifecycle: develop](https://img.shields.io/badge/lifecycle-develop-blue)
![R: not implemented](https://img.shields.io/badge/R-not_implemented-red)
![Python: partial](https://img.shields.io/badge/Python-partial-yellow)

## Intent

State the order for building output rows, deriving columns, and running
derivation expressions.

## Boundaries

This rule owns the two phases, dependency inference, and evaluation order.
R007 defines expression meaning. R002 defines name binding. R005 handles
finished expression results.

## Phases

**R001-1.** Derivation has two phases:

1. **R001-2.** Row construction evaluates `rows` entries and may change row
   count.
2. **R001-3.** Column derivation enriches constructed rows and must not
   change row count.

**R001-4.** Each `rows` entry builds output rows from one input dataset, named
by `row.dataset`. When `root.input` declares exactly one dataset, an entry
omitting `dataset` builds from that dataset. When `root.input` declares
more than one, every entry must state `dataset`.

**R001-5.** A row template has one of two modes:

1. **R001-6.** A row template without `group_by` is record-driven. Its
   `filter`, when present, evaluates against each input record before any
   row derivation. Every retained input record produces one candidate row.
2. **R001-7.** A row template with `group_by` is group-driven. Its
   non-empty list names only qualified variables of the row template's input
   dataset. Each variable's type defines equality; R019 defines strings.
   Missing values equal missing values. Every group produces one candidate row.

**R001-8.** Groups are ordered by the position of their first input record.
Within a group, records retain input order. For each group, evaluate every
row derivation once and complete stages 1 through 4 of the R005 lifecycle.
Then evaluate the row template's `filter`, when present, over the candidate's
completed unqualified columns. Append the candidate only when the `filter`
is `TRUE`; `FALSE` or `UNKNOWN` suppresses the candidate. A grouped
`filter` filters after a group reduction. An ungrouped `filter` filters
input records.

**R001-9.** Constructed rows are appended in specification order, using input
order for record-driven row templates and first-occurrence group order for
group-driven row templates.

**R001-10.** The input datasets and the row templates fix the output row
grain. No operation repeats a candidate a data-dependent number of times.
No generated index supports such a repetition. A source value may decide
whether a written row template retains its one candidate. That value
cannot create additional instances of that row template.

**R001-11.** When the required artifact has one row per observation,
administration, or planned event, an input dataset must contain one input
record per required row. Expected-but-uncollected rows use an explicit
planning relation at that grain and may be enriched from collected
relations through record lookups. Dynamically counted expansion must happen
upstream. The expanded records enter the specification as ordinary input.

**R001-12.** The `keys` state the output grain, and `keys` must be
declared. When `rows` is absent or empty, row construction derives the
distinct combination of `keys` over the input records, in first-appearance
order, and that key table is the output row set. The key table is standalone:
one row per unique key combination, with no link back to the input records,
so the input records a key combination was derived from decide its column
values and never how many rows the artifact carries. `base` is required
in that case, unless `input` declares exactly one dataset, which
supplies the input records.

**R001-12a.** When `rows` is present, row templates construct the rows.
Each row template is one section. A row template's `filter` keeps input records
or candidate groups. Each retained input record or group yields one row.
The sections concatenate in specification order. Row templates build a grain
finer than input records only as R001-10 permits. The built grain must
still be the `keys` grain. Repeating a key combination fails at the output
gate under R005-52. A `filter` states which rows the artifact carries, never
which input record represents a key combination. A row template that keeps
one of several input records with one key combination writes the `keys`
grain. The specification omits `rows` instead.

**R001-12b.** A column derivation must yield exactly one value per row, and
the derivation counts values rather than the records carrying them:
repeated readings of one value are that one value, and two input records
of one key combination carrying different present values are two values,
which fails under R001-44. A source `filter` decides which of those
records the derivation reads before that count, which R003-21 defines. A
missing result is still the row's one value but never creates a second
value for the R001-44 count. In a specification without `rows`, a key
column derivation must not depend on a non-key output column (R001-43);
keys are derived before any row logic runs.

## Expression evaluation

**R001-13.** An expression contains exactly one keyword registered by R007.
Most keywords name their input variables directly. Resolve those variable
dependencies, then evaluate the keyword. Fields whose declared type contains
`expression` are evaluated recursively. A `source` or `literal` expression is
a leaf. YAML mapping order has no execution meaning.

**R001-14.** Window expressions evaluate over the partitions declared by their
own `group_by`. Aggregate expressions evaluate in the contexts R007 permits.
All other expressions return one value per current row.

**R001-15.** During group-driven row construction, a source variable of the
template's input dataset is a scalar only when that exact qualified variable
occurs in the row template's `group_by`. An aggregate expression may instead
reduce the records of the current group under R007 and R013. Other row
expressions consume group keys, literals, earlier row-derived columns, or a
record lookup with already-complete matching values. That consumption
follows dependency order.

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
`row.filter` resolves only qualified variables of that row template's input
dataset and runs before that row template's derivation graph. A grouped
`row.filter` resolves unqualified columns derived by that row template and
runs after that graph completes. Identifier extraction requires parsing the
predicate under the R004 grammar, the numeric expression under the R010
grammar, the string template under the R012 grammar, and the reducer
expression under the R013 grammar. An implementation must not treat any of
those four as dependency-free.

**R001-27.** For each row template, evaluate row derivations using a
dependency graph. Row derivations cannot depend on values produced only
during the column phase. Every unqualified identifier in a grouped
`row.filter` must resolve to a column derived by that same row template. That
grouped `filter` is not a derivation and adds no graph edge between columns.
That grouped `filter` runs only after all columns have completed.

**R001-28.** After row construction, build the column dependency graph. Every
dependency must refer to a column declared earlier. Evaluate columns in
declaration order. When a specification declares `parents`, R017 composes,
prunes, and orders the resolved columns before R001 applies.
Declaration order is the resolved order.

**R001-29.** The column dependency graph is over columns, not over rows. A
column that reads another row of its own partition therefore depends on the
whole named column. A column that reaches its own value through another row
is a cycle rather than an iteration. `previous_non_missing` crosses any
number of missing rows by searching a separate completed source column.
Conventional carry-forward coalesces the current source with that search
result. Searching the column being derived remains a cycle rather than an
instruction to iterate.

**R001-30.** In both phases, a completed derivation runs the R005 lifecycle
before anything depends on that derivation. A dependent always reads a value
of the declared type.

**R001-31.** `output.columns` selects and orders artifact columns
independently of declaration order, and `output.order_by` orders the
artifact's rows independently of the construction order this rule defines.
R005 owns both.

## Rationale

Row count changes only during row construction. A reviewer can therefore
separate row grain from enrichment. The row templates and their input
datasets fix how many rows exist before any column is derived. Dependency
inference makes declaration order checkable and cycles visible.
Evaluation order never follows mapping order or repeated reads of a partition.

## Errors

- **R001-32.** A `rows` entry omitting `dataset` when `root.input`
  declares more than one: fail.
- **R001-33.** A specification with no `rows` entry and no default input
  dataset: fail. The default is root `base`, or the single declared dataset
  when `base` is omitted.
- **R001-34.** An empty or duplicate `row.group_by`: fail.
- **R001-35.** A `row.group_by` variable not qualified to that row
  template's input dataset: fail.
- **R001-36.** A grouped row derivation reading a non-grouped source variable
  without an aggregate: fail and report the variable.
- **R001-37.** An ungrouped `row.filter` naming an output column, or a grouped
  `row.filter` naming a qualified variable or a column not derived by that
  row template: fail.
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
  combination: fail and report the column, how many values it yielded, and
  the keys. Missing results are excluded from the count. A source declaring
  `multiple_matches` keeps one of the records carrying those values instead
  of failing, which R008-12 defines.
