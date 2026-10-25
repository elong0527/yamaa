# Inputs

An input dataset is a stored file a specification reads. Its path selects the
profile that reads it, its records keep their stored order, and each of its
fields has one type. No profile, type or missing value is inferred from the
file's contents.

## Profiles

<a id="input-1"></a>

### INPUT-1. The input path's extension selects the profile that reads it, and nothing is sniffed

`dataset_class.path` names the file a specification reads, and its
case-insensitive extension selects the source profile that reads it,
including for an input linked to its producing specification: `.csv`,
`.parquet`, `.sas7bdat`, `.xpt`, `.json` or `.ndjson` for a Dataset-JSON 1.1
file, or `.xml` for an ODM 1.3 or 2.0 document
read into the item table of [NAME-8](names.md#name-8). No reader inspects a
file's contents to choose a profile, a delimiter or a quote character, and no
unknown extension falls back to a default: it fails during validation, before
any byte is read.

Fails: `source_profile_unknown`.
Evidence: `negative-source-unknown-format`, `schema-parquet`, `sdtm-dm-odm-snapshot`, `negative-odm-transactional`, `negative-odm-no-clinical-data`, `rust/crates/yamaa-adapters/tests/table_sas_source.rs`, `rust/crates/yamaa/tests/odm_input.rs::nothing_is_sniffed_so_an_archive_named_xml_is_refused_as_xml`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/dataset_json_sources.json::a_single_object_file_reads_each_data_type_as_the_type_its_table_gives`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/dataset_json_sources.json::a_single_object_written_over_several_lines_is_no_newline_delimited_file`, `schema-source-dataset-json`, `schema-source-dataset-ndjson`.

The extension selects the profile under
[OUTPUT-5](output.md#output-5); a study that stores `DM.CSV`
names the same container as one that stores `dm.csv`.

| Extension | Profile | Container | Source-profile owner |
|---|---|---|---|
| `.csv` | `csv` | delimited text | [CSV profile](reference/csv.md) |
| `.parquet` | `parquet` | Apache Parquet | [Parquet profile](reference/parquet.md) |
| `.sas7bdat` | `sas7bdat` | SAS dataset (read-only) | [SAS source profiles](reference/sas.md) |
| `.xpt` | `xpt` | SAS Transport v5/v8 (read-only) | [SAS source profiles](reference/sas.md) |
| `.xml` | `odm` | ODM 1.3 or 2.0 XML (read-only) | [ODM source profile](reference/odm.md) |
| `.json` | `dataset_json` | Dataset-JSON 1.1, single-object (read-only) | [Dataset-JSON source profiles](reference/dataset-json.md) |
| `.ndjson` | `dataset_ndjson` | Dataset-JSON 1.1, newline-delimited (read-only) | [Dataset-JSON source profiles](reference/dataset-json.md) |

A source profile follows the artifact profile: a file
[Output](output.md) writes and these rules read has
one profile name in both directions. The read-only SAS and Transport profiles
of [SAS-1](reference/sas.md#sas-1) and the read-only ODM profile of
[ODM-1](reference/odm.md#odm-1) are the exceptions; no artifact publication
profile corresponds to them.
The read-only Dataset-JSON profiles of
[DSJSON-1](reference/dataset-json.md#dsjson-1) are exceptions too: they carry
the names a submission's `transport` writes them under
([JSON-1](profiles/dataset-json.md#json-1)), but no artifact publication
profile corresponds to them.

An `.xml` input is one ODM document, never an archive: its bytes are read as
XML whatever they hold. A `.tar`, `.tar.gz` or `.tgz` path selects no profile,
so an archive's ODM member is extracted first, or read with the host function
`read_odm`, which reads archives.

A specification that reads an artifact another specification produced reads
it like any other input: the extension of its own `path` selects the profile.
The producing-specification link of [INPUT-10](#input-10) supplies the stored
fields, their order and their types, not the profile, and a Parquet artifact's
self-describing types must still equal them under [INPUT-11](#input-11).

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
Parquet, SAS, Transport, Dataset-JSON or ODM XML input dataset, appended after the stored
fields, whose names, order and types are unchanged. Its value is the one-based
position of the data record in stored record order, assigned before any
filter, join, grouping or window reads the input dataset; filtering keeps the
original positions. Without the declaration, ingestion adds no field.

Evidence: `schema-source-ordinal`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/declarative_case_predicates.json::source_ordinals_precede_filtering_and_count_multiline_records_once`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/declarative_case_predicates.json::an_ordinal_input_with_no_record_saves_its_header_alone`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/producer_links.json::a_linked_ordinal_is_local_to_the_consumer_and_never_part_of_the_contract`, `rust/crates/yamaa/tests/odm_input.rs::an_xml_input_no_odm_read_names_may_declare_an_ordinal`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/dataset_json_sources.json::a_generated_ordinal_counts_the_rows_from_one`.

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
Evidence: `negative-source-ordinal-collision`, `rust/crates/yamaa-application/tests/execution_declarative_case_predicates.rs::ordinal_collision_and_fixed_odm_schema_have_complete_public_diagnostics`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/declarative_case_predicates.json::an_ordinal_on_the_fixed_odm_schema_is_refused_before_capture`.

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

Evidence: `schema-parquet`, `rust/crates/yamaa-adapters/tests/fixtures/source_typing.json`, `rust/crates/yamaa-adapters/tests/table_sas_source.rs`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/typed_csv_sources.json`.

Two extracts of the same dataset bind the same field to the same type. Where
the type comes from depends on the container:

- A **SAS or Transport source** supplies character or numeric storage, with
  defaults and explicit numeric narrowing under
  [SAS source profiles](reference/sas.md).
- A **Dataset-JSON source** supplies each column's `dataType`, through the
  closed table and the explicit narrowing of
  [DSJSON-2](reference/dataset-json.md#dsjson-2).
- A **self-describing source** supplies it. The fixed item table of the ODM
  source profile ([ODM-1](reference/odm.md#odm-1)), a Parquet schema and an
  artifact's producing specification are each the field's type authority;
  the consuming specification never restates the type.
- A **typeless container**, such as a delimited text file, supplies none.

`dataset_class.types` declares each named field's type in a typeless
container, and any unnamed field is `str`. The declaration describes the
dataset as this specification reads it, so two specifications may read the
same delimited file with different declarations. A bare path is
[Schema language](reference/schema-language.md) shorthand for a
`dataset_class` with no `types`, and a mapping holding only `path` restates it
and fails as `redundant_long_form` under
[SCHEMA-15](reference/schema-language.md#schema-15); both of these are
declarations:

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
the source contract, including every field of a Parquet source; the
exceptions are explicit SAS numeric narrowing under
[SAS-3](reference/sas.md#sas-3) and Dataset-JSON narrowing under
[DSJSON-2](reference/dataset-json.md#dsjson-2). `types` must be absent
whenever `schema` is present, even when an entry agrees with the producer.

Fails: `unknown_field`, `redundant_field_type`.
Evidence: `negative-types-unknown-field`, `negative-redefined-date`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/producers.json::a_linked_input_types_no_field_its_producer_already_types`, `rust/crates/yamaa/tests/odm_input.rs::an_xml_input_declares_no_types`.

An unknown field fails with `unknown_field`, and a field whose type the source
already supplies fails with `redundant_field_type`. The producing
specification remains the only type authority of a linked input
([INPUT-10](#input-10)); `types` may be present only for a typeless source.
A Parquet input and an ODM XML input ([ODM-1](reference/odm.md#odm-1)) supply
the type of every field, so each of their `types` entries fails with
`redundant_field_type` during validation, before any byte is read.

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

### INPUT-8. A null or an empty field is missing, and no other text spells missing

In a delimited source, a field with no characters, bare or quoted, is missing
whatever its declared type, never a parse failure, and no type admits an
empty string from a delimited source ([INPUT-9](#input-9)). No text is a
missing-value sentinel: `NA`, `NULL`, `.`, `unknown` and every other spelling
are ordinary string values. What missing is, and how each operation treats
it, is [VALUE-25](values.md#value-25).

Evidence: `negative-source-na-age`, `rust/crates/yamaa-adapters/tests/table_csv_source.rs::collected_text_and_missing_remain_lossless`.

An empty field holds no text to parse. In a delimited source, no collected
empty value is distinct from a missing one. A typed container supplies missing
as its own null: a Parquet null ([PARQUET-8](reference/parquet.md#parquet-8)),
a SAS missing value ([SAS-5](reference/sas.md#sas-5)) and a Dataset-JSON
`null` ([DSJSON-3](reference/dataset-json.md#dsjson-3)).

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
Evidence: `rust/crates/yamaa-application/tests/fixtures/diagnostics/declarative_case_predicates.json::csv_present_empty_is_rejected_before_study_bytes_are_requested`.

Handlers answer for a collected empty string like any other string. The
container profiles keep their own distinction -- a Parquet null and a
zero-length string remain distinct under
[PARQUET-8](reference/parquet.md#parquet-8), as a Dataset-JSON `null` and
`""` do under [DSJSON-3](reference/dataset-json.md#dsjson-3) -- and the
convention applies at ingestion, after the profile decodes. An empty field of any other declared type
is missing under [INPUT-8](#input-8); a typed container admits no zero-length
string outside `str`. The delimited profile admits no empty string
([INPUT-8](#input-8)), so `present` fails with
`empty_string_present_unsupported`.

## Producing specifications

<a id="input-10"></a>

### INPUT-10. A schema link types a stored artifact from its producing specification without running it

`dataset_class.schema` names the complete yamaa specification that produces
the stored artifact at `path`. The link is type metadata only: the producer is
read and validated as a specification, and its output columns type the
artifact under [INPUT-11](#input-11), but it is never compiled or run, its own
sources are not read, and its own `schema` links are followed only for its
copy entries ([SPEC-9](specification.md#spec-9)). The consumer reads the
artifact like any other input, so the caller builds and saves the producer
first. A link cannot name the consuming specification.

Fails: `invalid_producer_contract`.
Evidence: `adam-results-ae-incidence`, `sdtm-dm-race-ethnicity`, `adam-adae-exposure-adjusted-incidence`, `schema-linked-csv-types`, `rust/crates/yamaa/tests/copy_entry.rs::a_copy_reads_a_producer_that_copies_in_turn_and_refuses_a_cycle`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/producer_links.json::a_consumer_of_a_csv_artifact_checks_against_its_producers_specification_without_data`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/producer_links.json::a_consumer_of_a_parquet_artifact_checks_against_its_producers_specification_without_data`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/producer_links.json::a_consumer_reads_the_csv_artifact_its_producer_saved_first`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/producer_links.json::a_consumer_reads_the_parquet_artifact_its_producer_saved_first`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/producer_links.json::a_consumer_reads_the_stored_artifact_after_its_producers_source_is_gone`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/producer_links.json::the_producing_specification_is_read_but_never_compiled_or_executed`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/producers.json::a_producer_whose_stored_field_has_no_label_is_no_valid_contract`.

```yaml
input:
  DM:
    path: input/dm.csv
    schema: input/dm.schema.yaml
```

`schema` is a `project_path` resolved like `dataset_class.path`, so
[File resolution](reference/files.md) confines both. The referenced
document is validated against the same `root_class` in `schema_spec.yaml`;
there is no second source-schema class or field-description language. A
document named by `schema` that does not validate as a complete yamaa
specification, whose `output.columns` do not form a valid output contract under
[INPUT-11](#input-11), or that is the consuming specification itself fails
with `invalid_producer_contract`. A missing producing specification fails like
any missing file, at `input.<dataset>.schema`.

A producer whose columns a `copy` entry declares has their types and labels
only through its own producer, so reading it resolves its copy entries
([SPEC-9](specification.md#spec-9)): each reads the output contract of the
producer its input links, in turn, and no other link, source or artifact of
the producer is read. A consumer's copy entries read its producers this way
before its layers compose.

Checking reads the producing specification but no study data, so it admits a
link whose artifact does not exist yet. Building the consumer reads the
artifact named by `path`: an absent artifact fails as a missing input at
`input.<dataset>.path`, and no producer runs in its place.

A project run is a caller that builds and orders several specifications. It
builds a producer in its run scope before the consumers that link it, and
hands each consumer the producer's held serialized artifact; a producer outside
the run scope is not built, and its consumer reads the saved file at `path`
([PROJECT-8](profiles/project.md#project-8),
[PROJECT-9](profiles/project.md#project-9)). Either way the link types the
artifact under [INPUT-11](#input-11).

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
Evidence: `adam-adsl-randomization`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/producers.json::a_stored_header_out_of_its_producer_s_output_order_mismatches_the_contract`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/producer_links.json::a_stored_header_in_another_order_mismatches_the_producer_contract`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/producer_links.json::a_stored_header_without_a_contract_column_mismatches_the_producer_contract`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/producer_links.json::a_stored_header_with_an_extra_column_mismatches_the_producer_contract`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/producer_links.json::a_stored_parquet_column_of_another_type_mismatches_the_producer_contract`.

A stored artifact whose field names, order or self-describing types do not
exactly match the producer's output contract fails with
`producer_contract_mismatch`. The comparison is made when the consumer reads
the artifact: a delimited header is compared before any cell is typed, and the
typed fields of every profile, including a frame a host supplies, are compared
before a generated ordinal is appended.

A stored cell in a delimited artifact is still text. After recognizing missing
values, ingestion applies the `str` row of [VALUE-3](values.md#value-3) to
every non-missing cell, and a producer column declared `date` or `datetime`
uses the forms of [VALUE-15](values.md#value-15), exactly as an inline `types`
declaration or a column conversion does. A Parquet artifact instead supplies
typed values under the [Parquet profile](reference/parquet.md), and the link does
not convert those values through text.
