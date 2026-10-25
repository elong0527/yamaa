# Name binding

Names connect a specification to its data. A qualified name reads a declared
input dataset or record lookup, a bare name reads an output column, and an
`odm` expression reads one collected item from an ODM input.

## Input datasets

<a id="name-1"></a>

### NAME-1. `input` declares every dataset a specification reads, each under its own identifier

`input` maps dataset identifiers to source declarations. `base`,
`rows.dataset`, qualified source variables and named intermediates refer to
datasets by these identifiers, and every referenced dataset identifier must
exist in `input`. A declaration is a path, with or without types for its
fields. A finished dataset from an earlier run is an ordinary source, declared
under its own name.

Fails: `unknown_field`.
Evidence: `rust/crates/yamaa-core/tests/fixtures/diagnostics/preflight.json::an_intermediate_reads_a_declared_input`, `rust/tests/host-fixtures/cases.json`.

A row template may also name a named intermediate under
[ROW-2](rows.md#row-2). [Inputs](inputs.md)
owns both declaration forms and their shorthand. A declared path is a
`project_path`: [File resolution](reference/files.md) fixes its written
form, its selected relative base and its readable bytes, and a source path it
does not accept fails under its condition. [Inheritance](inheritance.md)
preserves the path's origin when a declaration is inherited and rebases a
relative path in a materialized resolved specification
([LAYER-10](inheritance.md#layer-10)).

<a id="name-2"></a>

### NAME-2. No input dataset takes the output dataset's name

A dataset identifier must not equal the output `domain`; validation fails when
they match. No keyed construct reaches a sibling record of the output
dataset.

Fails: `duplicate_identifier`.
Evidence: `negative-source-self-reference`.

[Column derivation](columns.md) owns what happens when a column
reaches its own value through that column's window partition.

## Variables

<a id="name-3"></a>

### NAME-3. A qualified name reads a declared dataset or lookup, and a bare name reads an output column

The concise source form names one variable. `DATASET.VARIABLE` refers to
`VARIABLE` in the declared dataset or record lookup its qualifier names;
datasets and record lookups share one naming list. An unqualified reference
such as `AVAL` refers to a column of the output dataset. A reference to an
unknown dataset identifier or variable fails.

Fails: `unknown_field`, `unresolvable_name`.
Evidence: `schema-source-binding`, `negative-source-undeclared-field`, `rust/crates/yamaa-adapters/tests/fixtures/binding_diagnostics.tsv`.

```yaml
derivation: DM.SEX
```

[Lookup and joins](lookup.md) defines record lookup resolution. An
unresolved unqualified reference fails with `unknown_field`, or with
`unresolvable_name` and the suggested qualified spelling when the bare name is
a field of an in-scope dataset. A field typed `variable` holds a qualified
source variable or an unqualified constructed output column. The rule that owns
a field may narrow the qualified variables it accepts: a `between` value
qualified by an input dataset must name the current row's driver
([LOOKUP-9](lookup.md#lookup-9)), a key match value never names a lookup
([LOOKUP-5](lookup.md#lookup-5)), and a column-level window names a field
only of a driver that every row template has
([WINDOW-11](windows.md#window-11)). An unqualified name never falls back to a
field of the driver's record that no column takes, in a `between` value or a
key match value alike.

<a id="name-4"></a>

### NAME-4. A qualified read of the driver reads its current record, or in a group the group's key

A qualified reference to the current row template's driver reads the current
driver record; a qualified reference to another dataset follows
[Lookup and joins](lookup.md). During grouped row construction
there is no single current driver record: a qualified reference to the driver
is scalar only when the exact variable appears in the enclosing
`row.group_by`, and it then returns that group's key value. Reading any other
source variable with a scalar `source` fails.

Fails: `ungrouped_driver_field`.
Evidence: `schema-rows-group-scope`, `schema-column-default-own-record`, `rust/crates/yamaa-adapters/tests/fixtures/source_filter_scope.json`, `rust/crates/yamaa-adapters/tests/execution_grouped_driver_dataset.rs`, `rust/crates/yamaa-adapters/tests/fixtures/diagnostics/window_drivers.json`, `rust/crates/yamaa-adapters/tests/fixtures/diagnostics/column_driver_bindings.json`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/binding.json::a_column_reads_a_driver_field_its_grouped_templates_leave_out_only_through_an_aggregate`.

The driver is an input dataset or an eligible named intermediate under
[ROW-2](rows.md#row-2). A scalar `source` naming a source
variable absent from the `group_by` of the grouped row template that builds
the row it reads fails. A column-level derivation reads every constructed row,
so there a qualified reference to a row template's driver is scalar only when
every grouped row template driven by that dataset has the exact variable in
its `group_by`; otherwise it fails, naming the variable and that driver. A
column-level window spans the rows of every row template, so it also needs
every row template, grouped or not, to have that driver
([WINDOW-11](windows.md#window-11)). A grouped row reduces non-key variables
with an aggregate expression, as [Expressions](expressions.md) and
[Formulas](formulas.md) define.

The current row template of a column-level derivation is the one that built
the row. A derivation that completes the rows of row templates over several
drivers therefore reads, on each row, the driver record that built that row,
and joins any other input dataset on that input's declared `key`
([LOOKUP-1](lookup.md#lookup-1)): the same qualified name reads a record on
the rows its driver built and joins on the rows another driver built. A
derivation holding a qualified aggregate ([LOOKUP-18](lookup.md#lookup-18))
binds once for all of those rows instead: unless every row template has the
first row template's driver, it joins every input dataset, whichever driver
built the row.

<a id="name-5"></a>

### NAME-5. A `variable` field names exactly one variable, never an expression

An operand field typed `variable` accepts a concise source variable or a
constructed output variable; an operation cannot place a nested expression in
it, and operations compose through a named derived column. A `string_template`
placeholder binds its complete name as a `variable` field does, and text
outside placeholders is literal. Whether a plain string in any other field is
a name or text follows from its field's kind ([EXPR-5](expressions.md#expr-5)).

Evidence: `negative-variable-nested`, `rust/crates/yamaa-adapters/tests/fixtures/text_template_reference.json`.

```yaml
- name: PREFIX
  type: str
  derivation:
    str_extract:
      source: RAW.TEXT
      pattern: '^[A-Z]+'
- name: CATEGORY
  type: str
  derivation:
    mapping:
      source: PREFIX
      dict: {A: Alpha}
```

[Text](text.md) owns placeholder braces and escaping. A
field typed `regex` holds a regular expression applied to a string value under
[Text](text.md).

<a id="name-6"></a>

### NAME-6. A structured `source` adds handlers and a record filter to one variable

The `source` expression also accepts a mapping holding `variable` and local
binding or join behavior. `absent` and a source's `order_by` and `keep`
selection are handlers, which [Column derivation](columns.md) defines.
`filter` is not a handler: it states which records the source may read, and
reading no record is an absent match, which `absent` answers
([LOOKUP-24](lookup.md#lookup-24)).

Evidence: `schema-handlers`, `rust/crates/yamaa-adapters/tests/fixtures/source_filters.json`.

```yaml
source:
  variable: ADSL.TRTSDT
  absent: null
```

[Lookup and joins](lookup.md) defines the join uniqueness the
selection relaxes and the matched records `filter` narrows. `between` is not a
handler either: it keeps the matched records whose bounds hold a value of the
current row ([LOOKUP-9](lookup.md#lookup-9)).

## ODM item reads

<a id="name-7"></a>

### NAME-7. An `odm` expression reads one collected item named as `DATASET.ItemOID`

The `odm` expression reads one collected item from an ODM input. `item` names
the input and the item as `DATASET.ItemOID`. A dataset identifier holds no
period, so the qualifier ends at the first period and the rest is the
complete `ItemOID`, periods included. A bare string is the
[Schema language](reference/schema-language.md) shorthand for `{item: ...}`.

Evidence: `schema-odm-item-subject`, `schema-odm-form-items`, `rust/crates/yamaa-adapters/tests/fixtures/odm_reads.json`.

```yaml
derivation: {odm: ODM.IT.DM.AGE}
```

`event`, `form` and `item_group` each take a list of OIDs, written as a list
even when it holds one, and `filter` takes a predicate over the ODM input's
schema fields; [NAME-12](#name-12) states how they identify records and
[NAME-13](#name-13) what the read gives. A mapping holding only `item`
restates the bare string and fails as `redundant_long_form` under
[SCHEMA-15](reference/schema-language.md#schema-15).

```yaml
derivation: {odm: {item: ODM.IT.DM.SEX, event: [SCREENING]}}
```

<a id="name-8"></a>

### NAME-8. An ODM input binds its eleven fixed text fields by case-folded name, checked at validation

An ODM input is an input dataset an `odm` expression names. Its schema is
fixed: eleven fields, each of type `str`. A stored field binds to the schema
field whose name it equals under ASCII case folding. At validation, from the
CSV header or the Parquet schema and without reading a record, exactly one
stored field must bind to each schema field, and each bound field must be
stored as text; an `.xml` input's fields come from the ODM source profile.
An ODM input declares no `types` and no `schema`.

Fails: `odm_schema_field_ambiguous`, `odm_schema_field_missing`, `odm_schema_field_type`.
Evidence: `negative-odm-schema-missing`, `negative-odm-schema-ambiguous`, `negative-odm-schema-type`, `sdtm-dm-odm-snapshot`, `rust/crates/yamaa/tests/odm_input.rs::odm_reads_bind_the_item_table_of_an_xml_input`.

The fields are `StudyOID`, `MetaDataVersionOID`, `SubjectKey`,
`StudyEventOID`, `StudyEventRepeatKey`, `FormOID`, `FormRepeatKey`,
`ItemGroupOID`, `ItemGroupRepeatKey`, `ItemOID` and `Value`. Eight of them are
the hierarchy fields: `StudyOID`, `SubjectKey`, `StudyEventOID`,
`StudyEventRepeatKey`, `FormOID`, `FormRepeatKey`, `ItemGroupOID` and
`ItemGroupRepeatKey`. Every field but `Value` is an identifying field.

ASCII case folding maps `A` through `Z` to `a` through `z` and leaves every
other character unchanged. The binding does not rename the stored bytes, which
the storage profile reads as it always does; the specification spells the
schema's names. A stored field that binds to no schema field is a vendor
field: an ODM input does not expose it, and an `odm` expression never reads
it. An `.xml` input has no header to check: the
[ODM source profile](reference/odm.md#odm-1) builds its item table with each
schema field once, under the schema's own name and as text, so its binding
always holds, and the table's other columns are vendor fields. An ODM input
fails at validation with:

- `odm_schema_field_missing` when it lacks a schema field;
- `odm_schema_field_ambiguous` when it binds two stored fields to one schema
  field;
- `odm_schema_field_type` when it stores a schema field as anything but text,
  or declares `types` or `schema`.

The failure names the input and the fields.

<a id="name-9"></a>

### NAME-9. Every ODM input record carries the seven required identifiers

An ODM input's records are verified at ingest, before any expression reads
them: every record carries `StudyOID`, `MetaDataVersionOID`, `SubjectKey`,
`StudyEventOID`, `FormOID`, `ItemGroupOID` and `ItemOID`. The repeat keys and
`Value` may be missing. A record that lacks a required identifier fails at
ingest, naming the field and the first record that lacks it.

Fails: `odm_schema_value_missing`.
Evidence: `negative-odm-schema-value`.

<a id="name-10"></a>

### NAME-10. A row's ODM scope is the set of ODM input records the row was built from

A row's ODM scope is the set of ODM input records the row was built from, as
the table below fixes for each way a row is built. Equality on a hierarchy
field treats missing as equal to missing. `ItemOID`, `Value` and every field
outside the hierarchy never narrow a scope, so a row built for one item reads
the other items of its own item group occurrence.

Evidence: `schema-odm-item-sample`, `schema-odm-form-items`, `schema-odm-item-subject`, `schema-window-key-odm-records`.

| The row was built by | Its ODM scope |
| --- | --- |
| no `rows`, with the ODM input as the base | the records its key combination was derived from ([ROW-8](rows.md#row-8)) |
| no `rows`, with the ODM input as the base and a key derived by a window | the records equal to the row's one record on all eight hierarchy fields ([ROW-8](rows.md#row-8)) |
| a row template grouped over the ODM input | the records equal to the row on every hierarchy field in the template's `group_by` |
| a record-driven row template over the ODM input | the records equal to the driver record on all eight hierarchy fields |

Missing equals missing on a hierarchy field as it does under
[ROW-4](rows.md#row-4).

<a id="name-11"></a>

### NAME-11. An `odm` expression evaluates only for a row whose ODM scope covers the input it names

An `odm` expression evaluates for a row that has an ODM scope over the input
it names, in a column derivation or in a row derivation. The scope belongs to
the row: a column derivation reads, for each row that evaluates it, the scope
of the row template that built it. An `odm` expression in a named
intermediate, or one that a row with no ODM scope over its input would
evaluate, fails at validation.

Fails: `invalid_odm_context`.
Evidence: `negative-odm-context`, `schema-odm-column-override`.

A row template that overrides a row-local column derivation does not evaluate
that column's `odm` expression ([SPEC-6](specification.md#spec-6)). An `odm`
expression in a named intermediate has no scope, and neither has one that a
row built from another dataset would read.

<a id="name-12"></a>

### NAME-12. An `odm` read identifies the scope records of its item that meet its OID lists and filter

The read identifies the records of the row's scope whose `ItemOID` is the
named item, whose `StudyEventOID`, `FormOID` and `ItemGroupOID` are among the
OIDs that `event`, `form` and `item_group` list, when they list any, and for
which `filter` is `TRUE` under [Formulas](formulas.md).
`filter` names only schema fields, qualified with the ODM input.

Evidence: `schema-odm-form-items`, `schema-odm-item-subject`, `rust/crates/yamaa-adapters/tests/fixtures/odm_reads.json`.

<a id="name-13"></a>

### NAME-13. An `odm` read gives the identified record's `Value`, missing for none, and fails for several

No identified record gives missing. One identified record gives its `Value`,
which may itself be missing. Two or more identified records fail. Values take
no part in identification: two records that carry one value are still two.
The read is a lookup, not a reduction; nothing is counted, ordered or chosen.
The result is `str` and converts to the column's declared type at completion
under [Values](values.md), as any derivation result does.

Fails: `odm_not_unique`.
Evidence: `negative-odm-not-unique`, `negative-odm-repeated-record`, `schema-odm-item-subject`.

The failure reports:

- the row;
- the item;
- the number of records;
- each identifying field whose values differ among them, or that no
  identifying field tells them apart.
