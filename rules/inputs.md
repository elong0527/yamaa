# Inputs

An input dataset is a stored file a specification reads. Its path selects the
profile that reads it, its records keep their stored order, and each of its
fields has one type. No profile, type or missing value is inferred from the
file's contents.

## Profiles

<a id="input-1"></a>

### INPUT-1. The input path's extension selects the profile that reads it, and nothing is sniffed

`dataset_class.path` names the file a specification reads, and its
case-insensitive extension selects the source profile that reads it:
`.csv`, `.parquet`, `.sas7bdat` or `.xpt`. An input linked to its producing
specification instead takes the profile its producer wrote. No reader inspects
a file's contents to choose a profile, a delimiter or a quote character, and no
unknown extension falls back to a default: it fails during validation, before
any byte is read.

Fails: `source_profile_unknown`.
Evidence: `negative-source-unknown-format`, `schema-parquet`, `rust/crates/yamaa-adapters/tests/table_sas_source.rs`.
Replaces: REQ-0751, REQ-0830, REQ-0831, REQ-0832, REQ-0852.

The extension selects the profile under
[OUTPUT-5](output.md#output-5); a study that stores `DM.CSV`
names the same container as one that stores `dm.csv`.

| Extension | Profile | Container | Source-profile owner |
|---|---|---|---|
| `.csv` | `csv` | delimited text | [CSV profile](reference/csv.md) |
| `.parquet` | `parquet` | Apache Parquet | [Parquet profile](reference/parquet.md) |
| `.sas7bdat` | `sas7bdat` | SAS dataset (read-only) | [SAS source profiles](reference/sas.md) |
| `.xpt` | `xpt` | SAS Transport v5/v8 (read-only) | [SAS source profiles](reference/sas.md) |

A source profile follows the artifact profile: a file
[Output](output.md) writes and these rules read has
one profile name in both directions. The read-only SAS and Transport profiles
of [SAS-1](reference/sas.md#sas-1) are the exceptions; no artifact
publication profile corresponds to them.

A specification that reads an artifact another specification produced learns
its byte encoding from the producer through the producing-specification link
of [INPUT-10](#input-10). The producer's `output.path` states the profile by
its extension, just as `output.columns` states the fields, and the consumer
reads the profile from the producing specification, not from the name it knows
the file by. A copy stored under another name is still read under the profile
its producer wrote it with.

`source_profile_unknown` is decided from the written path before any byte is
read and reports in the `validation` phase. Source-profile reading conditions
are decided while the snapshot is read and report in the `ingest` phase.

## Records

<a id="input-2"></a>

### INPUT-2. A reader returns records in stored order, and nothing reorders them

Stored record order is part of the input contract. A reader returns records in
stored order after field decoding and typing; parallel reads, batches,
partitions and an engine's scan plan never reorder them, and filtering keeps
the relative order of the records that remain. This sequence is the
base-record and grouped-input order, breaks window ties and orders
floating-point reduction.

Evidence: `adam-adlb-ordered-sum`, `schema-source-ordinal`.
Replaces: REQ-0514, REQ-0515, REQ-0538.

The [Column derivation](columns.md) uses the sequence as
base-record and grouped-input order, [VALUE-23](values.md#value-23) uses it to
break window ties, and [Formulas](formulas.md) consumes it for
ordered floating-point reduction. No reader reorders records while reading,
decoding, typing, filtering or assembling parallel batches.

Record order is not a substitute for a business key or a declared sort. It is
the stable sequence of one artifact: replacing an artifact with the same
records in another order changes the input and can change an order-sensitive
result.

<a id="input-3"></a>

### INPUT-3. An input may declare a generated field holding each record's one-based stored position

`dataset_class.ordinal` optionally names a generated `int` field of a CSV,
Parquet, SAS or Transport input dataset, appended after the stored fields,
whose names, order and types are unchanged. Its value is the one-based
position of the data record in stored record order, assigned before any
filter, join, grouping or window reads the input dataset; filtering keeps the
original positions. Without the declaration, ingestion adds no field.

Evidence: `schema-source-ordinal`, `rust/crates/yamaa-adapters/tests/execution_declarative_case_predicates.rs::source_ordinals_precede_filtering_and_count_multiline_records_once`, `rust/crates/yamaa-adapters/tests/file_producer.rs::linked_producer_ordinal_is_local_to_consumers_and_keeps_the_stored_contract`.
Replaces: REQ-1291, REQ-1292, REQ-1293.

```yaml
input:
  SOURCE:
    path: input/source.csv
    ordinal: SourceOrdinal
```

The name is an `identifier`. The CSV header is excluded, and a quoted field
containing newlines still belongs to one record. Each input dataset numbers
its records independently; declarations reading the same stored file receive
the same positions. Row templates keep the input record's ordinal, including
when several templates read that record.

The generated ordinal is an ordinary input field for name binding, grouped and
record-driven row templates, predicates, aggregations, and window `group_by`
and `order_by`, and the existing rules for selecting a field from several
records apply. Ordering a `row_number` window by the ordinal produces dense
sequence numbers within each partition; the sequence restarts for each
partition while the input ordinals keep their stored positions. The ordinal is
emitted only through an explicitly derived output column that
`output.columns` selects. It is local to the consuming input declaration and is
not part of a linked producer's stored output contract.

<a id="input-4"></a>

### INPUT-4. A generated ordinal must not collide with a stored field or join an ODM input

An `ordinal` name equal to a stored field name fails validation with
`ordinal_field_collision`; ingestion never overwrites, renames or hides a
stored field to expose an ordinal. The check also applies to an empty input
dataset and a producer-linked input dataset. An input dataset used by an `odm`
expression must not declare `ordinal`: its exposed fields are the fixed schema
of [NAME-8](names.md#name-8), and the declaration fails
validation with `odm_schema_field_type`.

Fails: `ordinal_field_collision`, `odm_schema_field_type`.
Evidence: `negative-source-ordinal-collision`, `rust/crates/yamaa-adapters/tests/execution_declarative_case_predicates.rs::ordinal_collision_and_fixed_odm_schema_have_complete_public_diagnostics`.
Replaces: REQ-1294, REQ-1295.

`ordinal_field_collision` reports the input dataset, the generated field name
and the `input.<dataset>.ordinal` specification path. `odm_schema_field_type`
reports the input dataset, `declared: ordinal` and the
`input.<dataset>.ordinal` specification path.

## Field types

<a id="input-5"></a>

### INPUT-5. Every input field has one type, from its container or its declaration, never from its values

Every field of an input dataset has exactly one type drawn from `column_type`,
and every value bound from it carries that type. A field's type is a property
of the dataset, not of the values one extract holds. A SAS or Transport source
supplies character or numeric storage, a self-describing source supplies its
types, and every field of a typeless container, such as a delimited file, is
`str` unless `types` declares otherwise. A type is never inferred from values.

Evidence: `schema-parquet`, `rust/crates/yamaa-adapters/tests/fixtures/source_typing.json`, `rust/crates/yamaa-adapters/tests/table_sas_source.rs`.
Replaces: REQ-0516, REQ-0517, REQ-0518, REQ-0519, REQ-0525, REQ-0537, REQ-1059, REQ-1060.

Two extracts of the same dataset bind the same field to the same type. Where
the type comes from depends on the container:

- A **SAS or Transport source** supplies character or numeric storage, with
  defaults and explicit numeric narrowing under
  [SAS source profiles](reference/sas.md).
- A **self-describing source** supplies it. An ODM `ItemDef` data type, a
  Parquet schema and an artifact's producing specification are each the
  field's type authority; the consuming specification never restates the type.
- A **typeless container**, such as a delimited text file, supplies none.

`dataset_class.types` declares each named field's type in a typeless
container, and any unnamed field is `str`. The declaration describes the
dataset as this specification reads it, so two specifications may read the
same delimited file with different declarations. A bare path is
[Schema language](reference/schema-language.md) shorthand for a
`dataset_class` with no `types`; both of these are declarations:

```yaml
input:
  EX: {path: input/ex.csv, types: {EXDOSE: float, EXSEQ: int}}
  DM: input/dm.csv
```

A declared type states what the study collects. A value that does not match it
is a defect in the data, not a reason to retype the field, and fails under
[INPUT-7](#input-7). No text is treated as absence ([INPUT-8](#input-8)).

<a id="input-6"></a>

### INPUT-6. A types entry may only type an existing field of a source that supplies no type

A `types` entry naming a field the stored dataset does not have, including the
generated ordinal, fails. A `types` entry for a field whose container or
producing specification already supplies a type fails rather than overriding
the source contract, including every field of a Parquet source; the one
exception is explicit SAS numeric narrowing under
[SAS-3](reference/sas.md#sas-3). `types` must be absent whenever `schema`
is present, even when an entry agrees with the producer.

Fails: `unknown_field`, `redundant_field_type`.
Evidence: `negative-types-unknown-field`, `negative-redefined-date`, `rust/crates/yamaa-core/tests/support/diagnostic/producer.rs`.
Replaces: REQ-0523, REQ-0532, REQ-0533.

An unknown field fails with `unknown_field`, and a field whose type the source
already supplies fails with `redundant_field_type`. The producing
specification remains the only type authority of a linked input
([INPUT-10](#input-10)); `types` may be present only for a typeless source.

<a id="input-7"></a>

### INPUT-7. Stored text parses into its field's declared type, or fails ingestion before any derivation

For a typeless container, the `str` row of [VALUE-3](values.md#value-3) parses
stored text into its field's declared type: `int` and `float` by numeric text
parsing under [VALUE-6](values.md#value-6), including its non-finite
normalization, and `date` and `datetime` by exactly the forms of
[VALUE-15](values.md#value-15). A value that does not parse fails the run,
reporting the dataset, field and value. This is an ingestion failure, not a
conversion failure: it occurs before any derivation, and no handler answers it.

Fails: `field_parse_failed`.
Evidence: `negative-ingest-unit`, `negative-source-na-age`, `rust/crates/yamaa-adapters/tests/fixtures/source_typing.json`.
Replaces: REQ-0526, REQ-0527, REQ-0536.

The YAML 1.2 non-finite spellings of [VALUE-6](values.md#value-6) remain text
when the field's type is `str`, and normalize to missing only after parsing as
numbers. `unconvertible` is declared on a result wrapper and answers for a
value a derivation produced, as the
[Column derivation](columns.md) and
[Column derivation](columns.md) define. A specification that wants to
see a value that does not match its declared type declares the field `str` and
converts it at the consuming column, where a handler exists.

## Missing values

<a id="input-8"></a>

### INPUT-8. A missing value is recognized before typing, and no text spells it

A missing value is the absence of a value. It is recognized before typing,
every type admits it, and [Column derivation](columns.md) answer for
it. In a delimited source, a field with no characters, bare or quoted, is
missing whatever its declared type, never a parse failure, and no type admits
an empty string from a delimited source. No text is a missing-value sentinel:
`NA`, `NULL`, `.`, `unknown` and every other spelling are ordinary string
values.

Evidence: `negative-source-na-age`, `rust/crates/yamaa-core/tests/source_csv_precision.rs::collected_text_and_missing_remain_lossless`.
Replaces: REQ-0528, REQ-0529, REQ-0530, REQ-0531.

An empty field holds no text to parse. In a delimited source, no collected
empty value is distinct from a missing one.

<a id="input-9"></a>

### INPUT-9. Each input declares whether a stored empty string is missing or present

`dataset_class.empty_string` is `missing` or `present`, and an omitted
declaration means `missing`. Under `missing`, a stored zero-length string in a
`str` field is the missing value, recognized before typing, and no derivation
ever sees it as a value. Under `present`, the same stored value is the
collected empty string, a present `str` value. The convention applies to `str`
fields only. A delimited source must not declare `present`; the declaration
fails validation.

Fails: `empty_string_present_unsupported`.
Evidence: `rust/crates/yamaa-adapters/tests/execution_declarative_case_predicates.rs::csv_present_empty_is_rejected_before_study_bytes_are_requested`.
Replaces: REQ-1158, REQ-1159, REQ-1160, REQ-1161.

Handlers answer for a collected empty string like any other string. The
container profiles keep their own distinction -- a Parquet null and a
zero-length string remain distinct under
[PARQUET-8](reference/parquet.md#parquet-8) -- and the convention applies at
ingestion, after the profile decodes. An empty field of any other declared type
is missing under [INPUT-8](#input-8); a typed container admits no zero-length
string outside `str`. The delimited profile admits no empty string
([INPUT-8](#input-8)), so `present` fails with
`empty_string_present_unsupported`.

## Producing specifications

<a id="input-10"></a>

### INPUT-10. A schema link makes the producing specification an executable predecessor

`dataset_class.schema` names the complete yamaa specification that produces
the stored artifact, whose output contract and workflow provenance the
artifact then carries. The producer validates against the same `root_class`:
its sources must exist and validate, its derivations must be complete and its
own `schema` links validate recursively. It completes before the consumer
reads the artifact named by `path`. The dependency graph must be acyclic, so a
link cannot name the consuming specification or one above it in the workflow.

Fails: `invalid_producer_contract`, `producer_workflow_cycle`.
Evidence: `adam-adae-exposure-adjusted-incidence`, `sdtm-dm-race-ethnicity`, `rust/crates/yamaa-adapters/tests/file_producer.rs::study_relative_schema_links_select_each_producer_wrapper_and_its_output_base`.
Replaces: REQ-0520, REQ-0521, REQ-0534.

```yaml
input:
  DM:
    path: input/dm.csv
    schema: input/dm.schema.yaml
```

`schema` is a `project_path` resolved like `dataset_class.path`, so
[File resolution](reference/files.md) confines both. The referenced
document is validated against the same `root_class` in `schema.yaml`; there is
no second source-schema class or field-description language. A document named
by `schema` that does not validate as a complete yamaa specification fails with
`invalid_producer_contract`, and a producer dependency that creates a cycle
fails with `producer_workflow_cycle`.

<a id="input-11"></a>

### INPUT-11. A linked artifact's fields are exactly its producer's output columns, with their types

The producer's `output.columns` names every stored field exactly once and in
artifact order, and each selected entry of its `columns` supplies that field's
type and label; declared internal columns are not stored fields. A delimited
artifact's header and a Parquet artifact's schema fields must equal
`output.columns`, and a Parquet field's embedded type must equal the
producer's declared type. Missing, extra, reordered, duplicate or undeclared
fields fail.

Fails: `producer_contract_mismatch`.
Evidence: `adam-adsl-randomization`, `rust/crates/yamaa-core/tests/support/diagnostic/producer.rs`.
Replaces: REQ-0522, REQ-0524, REQ-0535.

A stored artifact whose field names, order or self-describing types do not
exactly match the producer's output contract fails with
`producer_contract_mismatch`.

A stored cell in a delimited artifact is still text. After recognizing missing
values, ingestion applies the `str` row of [VALUE-3](values.md#value-3) to
every non-missing cell, and a producer column declared `date` or `datetime`
uses the forms of [VALUE-15](values.md#value-15), exactly as an inline `types`
declaration or a column conversion does. A Parquet artifact instead supplies
typed values under the [Parquet profile](reference/parquet.md), and the link does
not convert those values through text.
