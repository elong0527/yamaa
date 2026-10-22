# Row construction

Row templates construct the output rows from declared input records or groups.
Without row templates, the output rows are the distinct key combinations of the
base records.

## Row templates

<a id="row-1"></a>

### ROW-1. Each row template has a unique id and builds its rows from one declared driver

Each row template has an `id` unique within `rows` and builds output rows from
one driver, named by its `dataset`: a declared input dataset or an eligible
named intermediate. When `input` declares exactly one dataset, a row template
that omits `dataset` builds from that dataset. When `input` declares more than
one, every row template must state `dataset`.

Fails: `driver_unavailable`, `duplicate_identifier`.
Evidence: `rust/crates/yamaa-core/tests/support/diagnostic/preflight.rs`, `rust/crates/yamaa-adapters/tests/fixtures/preflight.tsv`, `schema-rows-override`.

An omitted driver when no single input is implied, or a driver that names
neither a declared input nor a named intermediate, fails validation with
`driver_unavailable` at the row template's `dataset` field, naming the row and
its written driver, or missing when it is omitted. An intermediate's
eligibility is checked separately under [ROW-2](#row-2).

<a id="row-2"></a>

### ROW-2. An intermediate drives rows only when it reads an input and exposes typed fields

A named intermediate used as a row template's `dataset` exposes its source-only
filtered records, in source order, after its ordered `derivations` have
completed. The row template reads them through the intermediate's qualifier as
its current driver record, not through a keyed lookup. A driver intermediate
must read an `input` dataset and may not declare `key`, `between`, `order_by`,
`keep`, `absent` or `require_match`, and every derived field it exposes must
have one column type as its result type ([EXPR-14](expressions.md#expr-14)).

Fails: `invalid_intermediate_driver`, `unknown_intermediate_driver_type`.
Evidence: `rust/crates/yamaa-core/tests/support/diagnostic/intermediate_scopes.rs`, `rust/crates/yamaa-adapters/tests/fixtures/row_intermediate_reference.json`, `rust/crates/yamaa-adapters/tests/project_run.rs::callback_derived_fields_expose_their_declared_result_type_to_row_drivers`, `schema-intermediate-driver-columns`.

The intermediate's `columns` list, when present, limits the fields the row
template may read; otherwise all stored and derived fields are visible. Its
filter may read only its own donor fields and earlier derived names. Its
derivations may also read the records other intermediates select for each
donor under [LOOKUP-16](lookup.md#lookup-16). Neither its filter nor
its derivations may reference a current row. An ineligible intermediate fails
as `invalid_intermediate_driver` at the row template's `dataset` field.

An exposed derived field has a column type when it is typed, or when its
expression's result type under [EXPR-14](expressions.md#expr-14) is one
column type; a `source` reading another intermediate's column under
[LOOKUP-16](lookup.md#lookup-16) has that column's type. A `case` selecting an
`int` or a `str` and a Boolean `literal` have none, and an exposed derived
field without one fails row-driver validation as
`unknown_intermediate_driver_type`.

<a id="row-3"></a>

### ROW-3. A row template without group_by is record-driven and builds one candidate row per driver record

A row template has one of two modes. Without `group_by` it is record-driven:
its `filter`, when present, evaluates against each driver record's candidate
row after that record's derivations that do not read a window result, and reads
the candidate's derived columns and lookup state. Every driver record whose
`filter` is `TRUE` produces one candidate row; `FALSE` or `UNKNOWN` discards the
record before the window pass.

Evidence: `schema-row-templates`, `rust/crates/yamaa-adapters/tests/fixtures/row_filters.json`.

A row template with `group_by` is group-driven under [ROW-4](#row-4).
[ROW-6](#row-6) limits what a filter may name, and
[COLUMN-9](columns.md#column-9) fixes which of the values it reads are
complete.

<a id="row-4"></a>

### ROW-4. A row template with group_by builds one candidate row per group of driver records, then filters it

With `group_by` a row template is group-driven. `group_by` is a non-empty list,
without repeats, of qualified variables of the row template's driver. Each
variable's type defines equality, and missing values equal missing values.
Every group produces one candidate row. For each group, every row derivation is
evaluated once and completes stages 1 to 3 of [COLUMN-8](columns.md#column-8);
then the `filter`, when present, reads the candidate's completed unqualified
columns, and the candidate is appended only when the result is `TRUE`.

Fails: `invalid_field_type`, `unknown_field`.
Evidence: `schema-row-templates`, `schema-lifecycle`, `rust/crates/yamaa-adapters/tests/specification_sources.rs::row_group_declarations_and_shared_scope_keep_authored_diagnostics`.

[Values](values.md) defines equality for strings. Groups follow the
position of their first driver record, and driver order is kept within each
group. `FALSE` or `UNKNOWN` suppresses the candidate. A grouped `filter` thus
filters after a group reduction, while an ungrouped `filter` gates each driver
record's candidate row after its prewindow derivations ([ROW-3](#row-3)).

An empty or repeated `group_by` fails as `invalid_field_type`. A `group_by`
variable not qualified to the row template's driver fails as `unknown_field`.

<a id="row-5"></a>

### ROW-5. A grouped row derivation reads a driver field only as a group key or through an aggregate

During group-driven row construction, a source variable of the row template's
driver is a scalar only when that exact qualified variable occurs in the row
template's `group_by`. An aggregate expression may instead reduce the input
records of the current group. Other row expressions consume group keys,
literals, earlier row-derived columns, or a record lookup with already-complete
matching values, in dependency order.

Fails: `ungrouped_driver_field`.
Evidence: `negative-row-window-ungrouped`, `negative-row-ungrouped-second-template`, `rust/crates/yamaa-adapters/tests/fixtures/binding_diagnostics.tsv`, `rust/crates/yamaa-adapters/tests/execution_grouped_driver_dataset.rs`, `rust/crates/yamaa-core/tests/support/diagnostic/binding.rs`.

The aggregate reduces the group under
[Expressions](expressions.md) and
[Formulas](formulas.md). A grouped row derivation reading a
non-grouped source variable without an aggregate fails as
`ungrouped_driver_field`, naming the variable, the row template and that row
template's driver.

<a id="row-6"></a>

### ROW-6. A row filter names only columns its row template derives, and only values complete at its phase

An ungrouped row template's `filter` reads the candidate row's derived columns
and lookup state. It fails when it names a column the row template does not
derive, and fails validation when it names a column whose derivation reads a
window result, directly or through a lookup match: the filter evaluates before
the window pass. A grouped row template's `filter` fails when it names a
qualified variable or a column not derived by that row template.

Fails: `phase_boundary`.
Evidence: `rust/crates/yamaa-adapters/tests/fixtures/row_filters.json`, `rust/crates/yamaa-core/src/specification/types/tests.rs::a_deferred_lookup_match_has_its_authored_filter_phase_witness`.

A qualified grouped-filter reference or an unqualified filter reference outside
the row template's derivations fails with `phase_boundary`, naming the row,
identifier, available phase and required filter phase.

## Rows and their order

<a id="row-7"></a>

### ROW-7. Row templates are ordered sections, and each retained driver record or group yields one row

When `rows` is present, row templates construct the rows. Each row template is
one section, and each driver record or group its `filter` retains yields one
row. Constructed rows are appended section by section in specification order;
a record-driven section keeps driver record order and a group-driven section
keeps first-occurrence group order. Input datasets and row templates fix the
output rows: no operation repeats a candidate a data-dependent number of times.

Evidence: `schema-row-templates`, `adam-adex-uncollected`.

No generated index supports such repetition. A source value may decide whether
a written row template keeps one candidate; it cannot create more rows. Row
templates build rows finer than input records only as this rule permits.

When the required artifact has one row per observation, administration, or
planned event, an input dataset must contain one input record per required row.
Expected-but-uncollected rows use an explicit planning relation at those keys
and may be enriched from collected relations through record intermediates.
Dynamically counted expansion happens upstream, and the expanded records enter
the specification as ordinary input.

The built rows must still match the `keys`: repeating a key combination fails
at the output gate under [OUTPUT-3](output.md#output-3). A
`filter` states which rows the artifact carries, never which input record
represents a key combination. A row template that keeps one of several driver
records with one key combination still writes one row per key combination; the
specification omits `rows` instead.

## Rows without row templates

<a id="row-8"></a>

### ROW-8. Without row templates, the output rows are the distinct key combinations of the base records

The `keys` state the output row identity and must be declared. When `rows` is
absent or empty, row construction derives the distinct combinations of `keys`
over the input records, in first-appearance order, and that key table is the
output row set. Root `base` names the input dataset that supplies those
records; it is required unless `input` declares exactly one dataset, which then
supplies them.

Fails: `driver_unavailable`.
Evidence: `sdtm-se-subject-elements`, `schema-window-key-records`, `schema-window-key-odm-records`, `rust/crates/yamaa-core/tests/support/diagnostic/preflight.rs`, `rust/crates/yamaa-adapters/tests/execution_no_rows_record_reads.rs`.

The key table is standalone: it has one row per unique key combination and no
link back to the input records. The input records a key combination was
derived from decide its column values and never how many rows the artifact
carries.

A key derived by a window numbers the base records themselves, so each
retained record is its own key combination and builds one row. Every column
derivation then reads that one record, whatever operation reads it, as a
record-driven row template's derivation reads its driver record: a flag, a
`case` condition, `first_available`, a project function argument, and an
`odm` read, whose scope is the record's own item group occurrence
([NAME-10](names.md#name-10)).

A specification with absent or empty `rows` and no default input dataset fails
validation with `driver_unavailable` at `base`. The default is root `base`, or
the single declared dataset when `base` is omitted. An explicit `base` must
name a declared input dataset even when `rows` is present; an undeclared `base`
fails with the same condition and path, naming the written dataset. An
unavailable implicit default records a missing `row` in its context.

<a id="row-9"></a>

### ROW-9. A root filter selects base records before the distinct-keys step and reads only base fields

A root `filter` is the filter-only row template lifted to root. When `rows` is
absent, it selects the base input records for row construction before the
distinct-keys step of [ROW-8](#row-8). It reads only the base driver's fields
and evaluates before any derivation. A root `filter` declared together with
`rows` fails.

Fails: `phase_boundary`, `conflicting_row_construction`.
Evidence: `adam-adtte-dor`, `adam-adrs-response-records`, `rust/crates/yamaa-core/tests/support/diagnostic/preflight.rs`.

The base driver is the declared `base`, or the single declared dataset when
`base` is omitted. Unlike an ungrouped row template's `filter`
([ROW-3](#row-3)), a root `filter` reads only the base driver's fields. Like
any row template filter, it states which rows the artifact carries, never which
input the column derivations read. Explicit row templates and the lifted filter
cannot both drive row construction: a root `filter` with `rows` fails as
`conflicting_row_construction`.
