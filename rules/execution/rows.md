---
id: execution/rows
title: Row construction
status: normative
---

# Row construction

## Requirements

### Phases

<a id="req-0034"></a>

**REQ-0034.** Each row template builds output rows from one input dataset,
named by `row.dataset`. When `root.input` declares exactly one dataset, a
row template omitting `dataset` builds from that dataset. When `root.input`
declares more than one, every row template must state `dataset`; omission
fails validation.

<a id="req-0035"></a>

**REQ-0035.** A row template has one of two modes:

<a id="req-0036"></a>

**REQ-0036.** A row template without `group_by` is record-driven. Its
   `filter`, when present, evaluates against each input record before any
   row derivation. Every retained input record produces one candidate row.

<a id="req-0037"></a>

**REQ-0037.** A row template with `group_by` is group-driven. The
   `group_by` list is non-empty and names only qualified variables of
   the row template's input dataset. Each variable's type defines
   equality; [Text values](../values/text.md) defines strings. Missing values equal missing values.
   Every group produces one candidate row.

<a id="req-0038"></a>

**REQ-0038.** Groups follow the position of their first input record. Input
order is kept within each group. For each group, evaluate every row derivation
once and complete stages 1 to 3 of the [Execution lifecycle](lifecycle.md).
Then evaluate the row template's `filter`, when present, over the candidate's
completed unqualified columns. Append the candidate only when the `filter` is
`TRUE`; `FALSE` or `UNKNOWN` suppresses the candidate. A grouped `filter`
filters after a group reduction. An ungrouped `filter` filters input records.

<a id="req-0039"></a>

**REQ-0039.** Constructed rows are appended in specification order.
Record-driven row templates keep input order. Group-driven row templates
keep first-occurrence group order.

<a id="req-0040"></a>

**REQ-0040.** Input datasets and row templates fix output rows. No
operation repeats a candidate a data-dependent number of times. No generated
index supports the repetition. A source value may decide whether a written
row template keeps one candidate. The source value cannot create more rows.
A row catalog under [REQ-1249](rows.md#req-1249) is an authored specification
resource expanded into written-equivalent templates before input data is read.

<a id="req-0041"></a>

**REQ-0041.** When the required artifact has one row per observation,
administration, or planned event, an input dataset must contain one input
record per required row. Expected-but-uncollected rows use an explicit
planning relation at those keys and may be enriched from collected
relations through record intermediates. Dynamically counted expansion happens
upstream. The expanded records enter the specification as ordinary input.

<a id="req-0042"></a>

**REQ-0042.** The `keys` state the output row identity, and `keys` must be
declared. When `rows` is absent or empty, row construction derives the
distinct combination of `keys` over the input records, in first-appearance
order, and that key table is the output row set. The key table is standalone:
one row per unique key combination, with no link back to the input records,
so the input records a key combination was derived from decide its column
values and never how many rows the artifact carries. `base` is required
in that case, unless `input` declares exactly one dataset, which
supplies the input records.

<a id="req-0043"></a>

**REQ-0043.** When `rows` is present, row templates construct the rows.
Each row template is one section. A row template's `filter` keeps input records
or candidate groups. Each retained input record or group yields one row.
The sections concatenate in specification order. Row templates build rows
finer than input records only as [REQ-0040](rows.md#req-0040) permits. The built rows must
still match the `keys`. Repeating a key combination fails at the output
gate under [REQ-0240](../storage/publication.md#req-0240). A `filter` states which rows the artifact carries, never
which input record represents a key combination. A row template that keeps
one of several input records with one key combination still writes one row
per key combination. The specification omits `rows` instead.

<a id="req-1170"></a>

**REQ-1170.** A root `filter` is the filter-only row template lifted to
root: it selects base input records for row construction when `rows` is
absent, before the [REQ-0042](rows.md#req-0042) distinct-keys step. It reads
the base driver: the declared `base`, or the single declared dataset when
`base` is omitted. Its scope matches an ungrouped row template's `filter`.
Like any row template filter, it states which rows the artifact carries,
never which input the column derivations read.

### Catalog expansion

<a id="req-1249"></a>

**REQ-1249.** A row template may declare `catalog` to generate a fixed set of
ordinary row templates during specification resolution. `catalog.path` names a
CSV specification resource under the approved project roots. Its first record
is a header of unique identifiers; each later record has one present value per
header field, and at least one record is required. `catalog.id_column` names a
header field whose values are unique identifiers. Each generated row ID is the
template ID, an underscore, and that field's value. Generated templates appear
in CSV record order at the original template's position. They retain the
template's dataset, grouping, filter, derivations, and submission metadata.

The CSV profile parses the resource. Values are strings unless `catalog.types`
declares a field `int` or `float`; numeric cells must parse to finite values.
The catalog is specification source and contains ASCII only.
Each field named in `catalog.unique_columns` must exist and have no repeated
value across catalog records; the listed fields are checked separately.
In a template value, an entire scalar `${FIELD}` is replaced by that field's
typed value. Inside a predicate `filter` or `when`, `${FIELD}` is replaced by
its quoted string or numeric literal, with quote escaping, before the ordinary
predicate grammar validates it. Other embedded placeholders are invalid.
Every referenced field must exist in the catalog header. Expansion occurs
after inheritance composition and before column pruning and ordinary schema,
binding, and execution validation. An invalid path, CSV, ID, type, or
placeholder fails at the catalog field. The resolved specification contains
only the generated ordinary templates; execution never reads the catalog.

### Expression evaluation

<a id="req-0047"></a>

**REQ-0047.** During group-driven row construction, a source variable of the row
template's input dataset is a scalar only when that exact qualified variable
occurs in the row template's `group_by`. An aggregate expression may instead
reduce the input records of the current group under [Expression evaluation](../operations/expressions.md) and [Aggregation](../operations/aggregation.md). Other row
expressions consume group keys, literals, earlier row-derived columns, or a
record lookup with already-complete matching values. That consumption
follows dependency order.

### Interface behavior

<a id="req-1056"></a>

**REQ-1056.** The `row_class` fields have these meanings:

| Field | Meaning |
| --- | --- |
| `row_class.id` | Identifier of this row template, unique within rows. |
| `row_class.dataset` | Input dataset whose records build this entry's output rows; required when more than one dataset is declared. |
| `row_class.group_by` | Grouping keys over input records, producing one candidate row per group; [Execution lifecycle](lifecycle.md) defines grouped construction. |
| `row_class.filter` | Predicate selecting input records for an ungrouped row template or completed candidate groups for a grouped row template. |
| `row_class.derivations` | Columns this row template derives; [Specification structure](../specification/structure.md) owns coverage across row templates. |
| `row_class.catalog` | Fixed CSV catalog expanded into ordinary row templates under [REQ-1249](rows.md#req-1249). |
| `row_class.submission` | Per-value submission metadata for this template's values, keyed by column; [Submission metadata](../submission/metadata.md) owns the declaration rules. |

## Error conditions

<a id="req-0063"></a>

**REQ-0063.** Retired; see [REQ-0034](#req-0034).

<a id="req-0064"></a>

**REQ-0064.** A specification with no `rows` entry and no default input
  dataset: fail. The default is root `base`, or the single declared dataset
  when `base` is omitted.

<a id="req-0065"></a>

**REQ-0065.** An empty or duplicate `row.group_by`: fail.

<a id="req-0066"></a>

**REQ-0066.** A `row.group_by` variable not qualified to that row
  template's input dataset: fail.

<a id="req-0067"></a>

**REQ-0067.** A grouped row derivation reading a non-grouped source variable
  without an aggregate: fail and report the variable.

<a id="req-0068"></a>

**REQ-0068.** An ungrouped `row.filter` naming an output column, or a grouped
  `row.filter` naming a qualified variable or a column not derived by that
  row template: fail.

<a id="req-1171"></a>

**REQ-1171.** A root `filter` declared together with `rows`: fail. The
  filter is the filter-only row template; explicit row templates and the
  lifted filter cannot both drive row construction.
