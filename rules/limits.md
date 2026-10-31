# Resource limits

One fixed policy bounds what each operation admits, the same for every Python,
R and command-line call. These rules state the limits, how a refusal reads,
and which table a partitioned run's row and cell limits bound.

## Refusals

<a id="limit-1"></a>

### LIMIT-1. A size past a fixed limit refuses its operation as engine_rejected, naming the boundary, its limit and the observed size

A size or count past a limit of [LIMIT-2](#limit-2) refuses the operation that
reaches it as `engine_rejected`, citing no rule
([COLUMN-15](columns.md#column-15)), at the specification paths it concerns.
Its context holds `step`, the lifecycle step; `code`, the kind of boundary;
`resource`, the boundary's name; `limit`, the fixed maximum; and `actual`, the
observed size when it is known. The refusal stops the operation before its
later effects, and the findings completed before it are kept.

Fails: `engine_rejected`.
Evidence: `rust/tests/host-fixtures/cases.json::column-ceilings`, `rust/tests/host-fixtures/cases.json::pattern_beyond_the_compiled_size_limit`, `rust/tests/host-fixtures/cases.json::a_pair_relation_key_beyond_the_key_limit`, `rust/crates/yamaa-application/tests/file_workflow.rs::per_source_resource_and_decoder_limits_keep_original_facts_without_partial_diagnostics`.

`code` is `compiler_limit` for a declaration counted while a specification is
prepared, such as its columns, inputs or keys, and a code of its own, such as
`resource_limit` or `output_limit`, for a file, a table or a saved artifact.
`resource` names what was counted, such as `spec_bytes`, `table_rows`,
`columns`, `text_bytes` or `regex_size`. A check reports the preparation
refusals a build reports, without reading any input record.

<a id="limit-2"></a>

### LIMIT-2. One fixed policy bounds each file, table, declaration, pattern and built text, and no caller selects another

Each file, table, declaration, compiled regular expression and built text value
is admitted against the fixed maximum below at the boundary that owns it,
before it is kept or grown. No caller selects other limits, no budget is shared
between boundaries, and no limit depends on available memory.

Fails: `engine_rejected`.
Evidence: `rust/tests/host-fixtures/cases.json::column-ceilings`, `rust/crates/yamaa-core/src/limits.rs::fixed_size_and_structure_boundaries_admit_the_edge_and_refuse_the_next_value`, `rust/crates/yamaa-core/src/limits.rs::table_geometry_is_checked_without_allocating_rows_or_cells`, `rust/crates/yamaa-adapters/tests/table_sas_source.rs::a_small_file_cannot_decompress_to_more_than_the_source_byte_limit`, `rust/crates/yamaa-application/tests/file_workflow.rs::raw_schema_checks_keep_all_findings_and_public_checks_take_the_first_ten_thousand`.

| Boundary | Fixed maximum |
| --- | ---: |
| Each specification, schema, project, function, codelists or packaging-lock file | 8 MiB |
| Each encoded study source file | 10 GiB |
| Each saved output file | 10 GiB |
| Rows in a decoded or materialized table | 10,000,000 |
| Cells in a decoded or materialized table | 640,000,000 |
| Columns in a source, output or intermediate table | 1,024 |
| Keys in one key or grouping declaration | 64 |
| Inputs to one specification | 16 |
| Operands in one source selection | 64 |
| Links followed in one composition, each `parent` and each `include` entry | 8,192 |
| Compiled program of one regular expression | 10 MiB |
| One text value that `str_pad`, `str_decimal`, `str_template`, `str_replace` or `STR_JOIN` builds | 1 MiB |
| Findings in the final issue collection | 10,000 |
| YAML nesting depth | 64 |
| Expression and context nesting depth | 128 |

A MiB is 1,048,576 bytes and a GiB 1,073,741,824 bytes. A reader checks a
file's encoded size before keeping its bytes and its growth while reading; a
compressed SAS dataset is also refused when its live records times its row
length exceed the source file limit. A writer checks the saved file size before
extending it. A table's rows, columns and their product are checked before the
table is built or grown. Only the text the five verbs build counts against the
text limit ([TEXT-2](text.md#text-2)): text read from a source or copied is not
counted. A regular expression is refused when it compiles, never while it
matches ([TEXT-12](text.md#text-12)). The final issue collection keeps its
first 10,000 findings in their order.

## Partitioned runs

<a id="limit-3"></a>

### LIMIT-3. A build run over key-bounded partitions gives the whole-table run's result, and its row and cell limits bound each partition

Every Python, R and command-line build divides its driver input into
partitions of at most 624,640 records that never split a group of the
specification's first two declared keys, such as `STUDYID` and `USUBJID`, and
runs whole when it cannot divide it. A partitioned run gives the whole-table
run's rows, values, order, issues, run report and saved bytes. Partitions
change only what the row and cell limits bound: each partition of a divided
input and its output, and the whole table everywhere else.

Fails: `engine_rejected`.
Evidence: `rust/tests/host-fixtures/cases.json::partitioned-default`, `rust/tests/host-fixtures/cases.json::longitudinal-capacity`, `rust/crates/yamaa/tests/partitioned_corpus.rs::partitioned_and_streamed_runs_reproduce_every_benchmark_without_project_functions`, `rust/crates/yamaa/tests/partitioned_reading.rs::every_partition_safe_shape_reads_a_driver_input_beyond_the_row_limit_by_partition`, `rust/crates/yamaa/tests/partitioned_publication.rs::a_parquet_output_saved_in_partitions_that_divide_no_row_group_has_the_whole_tables_bytes`, `rust/crates/yamaa/tests/partitioned_publication.rs::the_output_of_a_sas_driver_beyond_the_row_limit_is_refused_as_the_whole_table_run_refuses_it`, `rust/crates/yamaa/tests/partitioned_confinement.rs::a_step_whose_records_no_partition_keeps_together_is_refused_unless_it_falls_back`.

A run divides only a CSV or Parquet driver input that is its specification's
only input, and only when no step needs the whole table. A step needs the whole
table when what it reads crosses partitions: a window, aggregate, grouped row
template or key grain whose grouping omits a partition key, a named or joined
relation, `SELF` rows or a project function. A specification that declares one
key partitions by it. A run of one key group longer than 624,640 records is a
partition of its own. The driver input runs as one partition when a row
template that reads it record by record does not copy each partition key
unchanged from one driver field, or when key values recur after their run of
records has ended. A window grouped by `[USUBJID, PARAMCD]` in a specification
keyed `[STUDYID, USUBJID, ...]` omits `STUDYID` and runs whole; grouped by
`[STUDYID, USUBJID, PARAMCD]`, it divides.

The row and cell limits bound each partition of a divided driver input, decoded
one at a time, and of its output, saved as CSV or Parquet one partition after
another with the whole-table write's bytes, Parquet row groups included. They
bound as whole tables the run report; a SAS, ODM or Dataset-JSON input and its
output; every input of a specification that reads more than one; the input and
output of a run with a step that needs the whole table; and a submission's SAS
Transport or Dataset-JSON file. So a run whose steps group by subject builds
and saves a CSV or Parquet input of any number of records, and a run that needs
the whole table is refused past the limits exactly as it is without partitions.

A Rust embedder's engine runs every build whole unless it sets
`EngineOptions::with_partitions`. Without the whole-table fallback, a step that
needs the whole table refuses the run as `engine_rejected` with `step`
`execute` and `code` `partition_global` at that step's path.
