# Installed host semantic fixtures

`saved-readers.json` independently owns the saved-format reader controls:
invented source records, declarations, exact CSV text, logical values, i64
decimal strings, IEEE 754 float bits and temporal epoch counts. The Python and
R `saved_readers` suites consume this same truth through installed public run
and save APIs; its `environment` key holds the project document both hosts
write as `project.yaml`. Both nonempty and header-only sources are qualified
for CSV, Parquet and their generated Dataset-JSON and Define-XML documents. No
benchmark input or expected artifact is regenerated.

`cases.json` owns independently authored inputs and expected truth repeated by
installed Python and R controls and, for its issue and run-report parity cases,
by the command line's tests ([The command line](#the-command-line)). The
readers only decode this data. They do not invoke a native engine, reference
evaluator, benchmark or callback to make expected values. Integer boundaries remain decimal strings; each host constructs
its original integer type. Text is UTF-8, and CSV comparisons retain all bytes,
including quoting, trailing zeros and final newlines.

The R assertions share `context.R` to compare decoded JSON contexts with key
order ignored at every level. Array order and scalar types remain exact.
`assert.R` holds the helpers every R suite shares: labelled expectations that
name the case and show the actual and expected values when they fail, D39's
save-refusal text, the issue-truth decoder and the PATH that hides Python.
`original.R` holds the helpers of the `original_specifications*.R` suites,
which split the original-specifications family by feature. Neither authors
truth; both only compare.

Each host's environments entry, lock, ABI handles and callbacks remain
host-owned. Public-contract data retains the different Python and R i64
vectors: R's plain doubles refuse the values beyond 2^53, while both hosts save
the held bytes.

| Paired family | Shared data and existing owners |
| --- | --- |
| original-specifications | Original case selection, lookup/ordering/casing inputs and exact CSV; existing independent TSV/JSON reports; `{entry}` in a namespace case's context stands for the specification's absolute path as the engine resolves it, with forward slashes |
| file-resources | Named lookup paths, issue columns, exact alias context and entry-relative programs that declare `output.report`; existing inspection/capture reports |
| public-environment | Original case selection, schema link references, quota/version inputs, exact callback CSV and the project-relative inherited and codelist files |
| verification-logs | `specification-original/schema-verification-log`, evaluated counts and fatal outcomes, now checked in the run report |
| public-define | Existing Define input/XML/JSON owners, creation time, issue columns, column schemas, numeric metadata variants, spec/project text mutations and the issue row of a lower-cased transport name collision (its context compared by the file name both writers would write) |
| public-contract | Shared YAML, issue and report columns, argument/member/read-only view names and defaults, shared messages (D39's error text, written from `docs/design/interface-1.0/api.md` and the templates of `yaml/conditions.yaml`; `{path}` stands for the specification or project path the test gave, and `{file}` for the file the first issue is located in, as its location names it), typed plain values, the failed run's report bytes, the issue rows of a save with nothing to write, and the value-level check rows of a run whose entry selects no row (SUB-13); distinct host i64 vectors |
| public-grammar | Shared condition families; existing `grammar-original/cases.tsv` decisions |
| diagnostic-parity | Refusal cases with their complete expected issue rows; `{path}` in a row's message and context stands for the specification path the test gave, and `{entry}` in its context for that specification's absolute path as the engine resolves it, with forward slashes |
| read-odm | The two reader documents (the adapters' `odm` fixtures), all 30 columns and cells, archive member selection, the authored sources two refusals read, and each refusal's one issue row and error text (`{source}` stands for the source path the test gave) |
| parquet-containment | Two invalid Parquet files, one whose INT96 statistic is too long and one whose row groups name one page with a header parquet 55 could not skip, their issue row, and a readable file with its saved CSV |
| parquet-writer-defaults | A Parquet file pyarrow 26 wrote with a timestamp in nanoseconds, one in milliseconds and a single-precision float, the specification that reads each, and its saved CSV |
| plan-reuse | One specification run A, B, A on one object with exact output CSV and issue rows; a changed specification's output; a changed function default's output |
| lint | One specification with layout, authoring and suppression findings: its exact `lint()` rows, the text the proved fixes write, the rows that remain, and three refusals, a named file written for 0.5 among them; a layer that restates its parent's row template without one derivation, with its `partial_restatement` row and a `restated_inherited_value` row for each derivation it writes as the parent does; and a layer that writes a derivation, a label and a copied column's label as its parent and the producer the parent copies from give them, beside a changed label and a derivation it clears, with its `restated_inherited_value` rows: each row written from the style guide, which `fix` leaves |
| selftest | The bundled cases in list order, the cases table's columns, the status vocabulary, and the report's lines and the layout of its information line |
| schema | The four file types, each JSON Schema's dialect, title and `$ref`, the canonical order of its top-level keys, and two refusals with their error text and issue rows |
| info | The engine information's fields in order, the host vocabulary, and a shipped build's language versions and features |
| conditions | The condition catalogue's seven columns, every condition it names in name order, and one authored row of each shape: rules in card order, a condition whose issues cite no rule, and the four renames and seven retirements of 0.6.0 |
| printing | A study's files, and the status and printed lines of each result: a check, a failed check, a project check, a run, a failed run, a project run and each of its members, a submission, and a lazy specification and project; `{path}` stands for the path the test gave and `{root}` for its study directory, with forward slashes |
| issue-locations | One case for each rule of P-ISSUE-1 (`docs/design/interface-1.0/api.md`, "Issue locations"), and for a refusal at the `parent` or `include` entry of a layer above the entry, which is located in that layer (D181): the files a case writes, its call, and its first issue's condition, first spec path, file, line and column, counted by hand in those files |
| include-fragments | One study whose entries compose a parent chain and an `include` list (LAYER-1 to LAYER-4): the files it writes, each check case's entry and complete issue rows, each parent case's fragment, given parent and child, whose two checks return its rows, each refused parent's error text and rows (#3694), and the resolve case's layers, `parent` and `include` resources, resolved columns, the layer of each named value and the fields the resolved specification never holds; `{root}` stands for the study's directory, `{path}` in an error text for the path the test gave and `{parent}` for the parent it gave |
| project-parity | Project cases with their complete expected issue rows: the files a case writes, its call (a project check, a member of a project run, a specification the project names, or a submission) and every issue row, or under `calls` each of several calls on those files with its rows; rows and files that depend on the host language are given by language, `{root}` stands for the case's directory and `{version}` for the installed yamaa version |
| compact-yaml | A specification with a block and a flow input, the block, compact, flow and explicit-key verification forms with their one output and saved CSV, failed row-count bounds, malformed closers, and raw NULs with their `invalid_yaml` context (#2196, #3125) |
| longitudinal-capacity | The 50,000-row, 100-column profile: its column names, its value rule (the row index for the key, then an offset plus the row and column index modulo a prime), the size its CSV must exceed, its source and output formats, and how every run of it divides its input under the hosts' default partitions (#3794) |
| partitioned-default | A study of 10,000,001 records, beyond the row limit, keyed by subject: its specification, whose window numbers each subject's records, its input header and value rule (record `i`, counted from 0, is study `S1`, subject `i` divided by `subject_records` in eight digits, and `SEQ` the remainder plus 1), and the line count and first and last lines of its saved CSV (#3794) |
| column-ceilings | The 1,024 admitted and 1,025 refused widths of each boundary, the compiler-limit and parent-occurrence issue rows, the repaired capacity's remaining error, the inheritance depth that builds and the wide inherited cases' rows by column index modulo 3 |
| interface | The generated request, each crafted request with its authored outcome (the accepted record or the refusal envelope), the message template of each reason and the gate's refusal, where `{version}` stands for the boundary version, `{next_version}` for the version after it and `{core_version}` for the installed engine's release; the census kinds; and the lifetime study's specification, input, project and output rows |
| streaming | A study of every column type: its specification, whose output orders the records by descending ID, its input, the output rows in that order and the exact CSV its save writes; a 70,000-record study with its value rule (the ID counts from 1, and the name is the prefix and the ID) and the batches the engine's 65,536-record batches give it; the class of the stream object R returns; and D39's text of a cancelled streamed run and of its `stream_taken` and `host_thread` refusals |
| progress | A study whose save writes an output and a report file: its specification and input, the progress events a run of it reports in order (each phase of a run without project functions started and ended, with the one key-bounded partition of its two records that the hosts' default partitions give it entered in derivation, `{path}` standing for the path the test gave), the timings table's columns and the phases a run and a check list, and the issue rows of a run whose progress callback raised (#3762) |

`plan-reuse` is the repeated-run truth for #2821. A spec object's first
`run()` compiles and later runs reuse the plan, so study A, then B, then A on
one object must each give its own authored result: A's exact output CSV, and
B's `division_by_zero` row, the same row as `division_in_a_right_operand` in
`diagnostic-parity`. A run that leaked another study's output or issues would
differ. The changed specification adds 2 instead of 1, so A's output becomes
`3.5` and `2.25`. The function change sets `default: 1` for the `decimals` of
the `schema-functions` project function, so `RATIO_DEFAULT` rounds to one
place under ties-to-even (`0.33` becomes `0.3`, `2.25` becomes `2.2`) while
every column that passes `decimals` keeps the benchmark's expected value. The
Python and R `plan_reuse` suites run both changes on one object and require
the changed output, so a stale plan would fail them.

`diagnostic-parity` is the cross-host issue parity truth. Each case holds a
specification, its input, the call (`check` or `run`) and every row of the
issues table that call returns, in order: phase, condition, rule,
specification paths, context, source span and operand route. Both public
contract suites require their rows to equal this truth field for field, so the
two hosts equal each other. The context is the canonical JSON text both hosts
return (sorted keys, no whitespace, exact integer tokens), compared as text, so
no JSON reader chooses a host number type. A case whose specification is
absent names the path the test gave, so its rows write `{path}`, which each
suite replaces, as JSON text in the context; a finding in a layer names the
layer and the entry by the path the engine resolves, which the rows write
`{entry}`. Together the cases cover every
value shape a context holds: integers at and beyond 2^53, the signed 64-bit
bounds and a wider literal, fractional and whole floats, null, non-ASCII text,
text holding NUL (escaped), nested and empty objects and arrays, absent and
present geometry, an empty route, an absent rule, empty specification
paths and several rows. A zero-width span is not reachable from a
specification, because spans come only from expression nodes; the adapter
record tests own it. Spans are integer pairs, doubles in R, and both geometry
fields are null (`None`, `NULL`) when nothing was recorded.

An issue row's `file`, here and in `plan-reuse`, `parquet-containment`,
`project-parity` and `issue-locations`, names the file relative to the
directory the test writes the case in, and `line` and `column` were counted
by hand in that file (the issue locations of P-ISSUE-1). Each host compares
`file` with that file's absolute path, resolved as the engine resolves a
local path, with forward slashes; a row without a location holds null in all
three.

`project-parity` pairs the installed Python and R project parity suites,
which compare whole issue rows as `diagnostic-parity` does, for calls that
need a project: a project check, whose `resource_path_missing` findings name
each listed function or codelist file that does not exist at the field that
lists it, with the path as written (FILE-11), a member of a project run,
whose `unlinked_producer` and `producer_not_completed` findings name the
listed specification they belong to and the document they come from
(PROJECT-7), the run of a specification the project names, and a
submission, whose `duplicate_dataset_entry` refuses a section that lists two
specifications of one saved artifact (DEFINE-5). A case that makes several
calls on the same files lists each call with its rows under `calls`. Six
cases, from `a_key_that_is_not_an_output_column` to
`an_output_column_listed_twice`, each break one refusal of OUTPUT-1 or
OUTPUT-2 (an internal, undeclared or repeated key, an empty `keys`, and an
undeclared or repeated output column): each checks the project, runs the
specification it names and composes its submission, and every call reports
that one refusal at the same written position, at its spec path in the
specification's own calls (`keys[2]`) and at that path under the listing in
the submission's (`sdtm.specs[0].keys[2]`). A case whose files or rows
depend on the host language gives them by language: a function file that
implements only the other host refuses the call with the running host's
language (FUNC-1), and each host verifies only its own lock (FUNC-3). The
function files name standard-library callables (`operator.pos`,
`base::identity`), which no lock pins and which the R check environment
has, so neither host installs a project package for these cases. `{root}` stands for the case's directory,
resolved, with forward slashes, and `{version}` for the installed yamaa
version. The rows were written from those cards, the message templates of
`yaml/conditions.yaml` and the issue locations of P-ISSUE-1 before the
engine ran.

`include-fragments` pairs the installed Python and R fragment suites, and the
command line's parity test reads it too. Its study writes one template, two
fragments and an entry that names the template as its parent and lists both
fragments, beside the files of five refusals: a fragment that names a parent
(`prohibited_construct` at that fragment's `parent`, LAYER-4), a template
that includes the entry (`inheritance_cycle` at the template's `include[0]`),
a fragment that the template and the entry both include (`duplicate_layer` at
the entry's `include[0]`, LAYER-3), and an `include` entry that names no file
(`resource_path_missing`, FILE-7), written once in the entry and once in a
template under an entry that includes a fragment of its own, so that the
finding's `source`, the layer that writes the entry, locates it in the
template. The rows were written from those cards,
the message templates of `yaml/conditions.yaml` and the issue locations of
P-ISSUE-1 before the engine ran. The resolve case's layers follow LAYER-3's
contribution order, the template, then the fragments in list order, then the
entry, and its columns LAYER-13's: the template's, then the duration that
reads two of them, then the severity, each after what it reads and otherwise
by contribution position.

Its parent cases check a fragment with `check(parent=)`, and the child that
names that parent, includes the fragment and writes nothing else with
`check()`, and each pair returns the case's rows (#3694). The parent they are
checked on, `adae-template.yaml`, names the study's template as its own
parent and outputs `ADURN`. `duration.yaml`, which derives `ADURN`, checks
clean on it; `onset.yaml` derives `ADURN` from `AONSET`, which no layer
declares (`unknown_reference` at the fragment's `derivation`, COLUMN-5); and
`severity.yaml` leaves the `ADURN` that the parent outputs undeclared
(`undeclared_column` at the parent's `output.columns[2]`, OUTPUT-1). A parent
given for a layer that names a parent of its own, or for one that includes
fragments, is refused with `value_not_permitted` at `parent`, the argument,
which no file holds, its `value` the parent as given (LAYER-4), and D39's
error text. These rows too were written from the cards, the templates and
P-ISSUE-1 before the engine ran.

`read-odm` pairs the installed Python and R `read_odm` suites. Its documents
are the reader suite's own XML fixtures, staged from
`rust/crates/yamaa-adapters/tests/fixtures/odm` as `host-fixtures/odm`; the
expected cells were read from the documents by hand. `SourceOrdinal` is
decimal text in the truth (an integer in Python, a double in R) and every
other cell is text or null. The ASCII fixture adds event/form repeats and a
numeric reference to U+00E9 while preserving empty and explicitly null items;
both hosts also select it from the TAR archive. Each refusal holds the one issue row that
`yamaa.Error` or `yamaa_error` carries, and its error text is D39's prefix
and that issue; `{source}` stands for the source path the test gave. Its
`files` are sources that are neither document nor archive, which both
suites write beside the documents: a plain document under a `.tar` name,
which ODM-1 refuses as an archive name holding no archive, and a bzip2
stream, refused from its first bytes before anything is decompressed.

`parquet-containment` pairs the installed Python and R Parquet refusal
suites. It holds two invalid files, which share one issue row, and a readable
one. The first invalid file was authored field by field: the column chunk of
an INT96 column holds a 13-byte `min_value` statistic, where the statistic of
an INT96 column holds 12 bytes, and the footer refuses it. The second is the
readable file with the unknown set, maps, UUID and UUID list fields of the
adapters' reader tests in its page header, which parquet 55 could not skip,
and its row group repeated, so that two row groups name that one page and the
file declares four rows. parquet 55's path refused such a file once it
presented the header, and the native reader keeps that refusal, so the
admitted files do not change (D283). The engine refuses each file as one
`source_parquet_invalid` issue, and the adapters' reader tests check that
these bytes reach that refusal. The readable file holds the INT64 values 1
and 2, which a later run in the same process reads and saves. Each suite also
runs every specification in a fresh process and requires its stderr to be
empty. The family is named for what it first proved: that the engine
contained the panic of the upstream decoder on the INT96 statistic, which the
native reader refuses without one (#3878).

`parquet-writer-defaults` pairs the installed Python and R suites of the
Parquet types that pandas and pyarrow write by default (#4087). pyarrow 26.0.0
wrote its file, with its embedded Arrow schema and snappy pages, from a table
of an `ID`, a `NS` timestamp in nanoseconds, a `MS` timestamp in milliseconds
and a single-precision `F32`, none adjusted to UTC; each of the last three
holds one missing value. Each suite runs the specification, which reads every
field as its column type (PARQUET-7), requires no issue, and compares the CSV
it saves with `saved_csv`, byte for byte. The CSV was written by hand: the
nanosecond and millisecond counts are the whole seconds `2024-01-01T10:00:00`
and `1969-12-31T23:59:59` (PARQUET-9), and the single-precision value nearest
0.1 widens to the binary64 value whose shortest text is
`0.10000000149011612`.

`selftest` pairs the installed Python and R selftest suites. Its cases are
the list in `benchmarks/selftest.yaml`, and its lines are the printed lines of
#3030 D119 for a shipped build, which compiles every feature, so every case
passes, and a listed case covers every adapter, so no `not covered:` line is
printed. Each host fills the information line from its own facts: the
compiled core version, its interpreter's version, and the platform its
interpreter reports.

`schema` pairs the installed Python and R `schema()` suites (#3030 D149). Its
facts were read by hand from the JSON Schemas committed in `yaml/json/`,
and its refusal text was written from D39's prefix and the template that
`yaml/conditions.yaml` holds for `unexpected_file_type`. The suites check that
each schema is one ASCII line whose top-level keys come in key order, as
canonical JSON text keeps them; the facade's tests compare the whole text
with the committed files.

`info` pairs the installed Python and R `info()` suites (#3030 D41). Its
fields are those of `records.info` in `rust/interface/bindings.yaml`, and its
features and hosts that record's vocabularies, which a tools test checks. The
`selftest` suites read it for a selftest's `info`, which `info()` equals.

`conditions` pairs the installed Python and R `conditions()` suites (#3700);
the facade's catalogue tests and the command line's parity family read it
too. Its `names` are the keys of the `conditions` and `retired` sections of
`yaml/conditions.yaml`, in name order. Each of its `rows` was written by hand
from that file and the core registry: the phases and message template that a
registration gives and its `renamed_from`, or a retired condition's
`retired_in` with no phase, rule or message, and the card IDs that the
registry's causes of the condition cite, in card order (by prefix, then
number). A row is the list of its cells in the order of `columns`, so no row
reads as a parity case, which is an object with a `condition` key.

`printing` pairs the installed Python and R printing suites (#3030 D36,
D37, P-PRINT-1). Its lines were written from the forms of
`docs/design/interface-1.0/api.md` ("Printed text") and the message templates
of `yaml/conditions.yaml` before the engine ran. In its study ADSL builds,
ADAE fails its own check (COLUMN-5), and ADTTE reads ADAE through its
`schema` link and so is skipped; its submission is
`define-fixtures/sdtm-dm-metadata`, with the creation time of
`public-define`. Each `first issue:` line names its location as D181 gives
it, counted by hand in the study's files: ADAE's at its `derivation` key,
ADTTE's at its `schema` key. The study lies outside the current directory,
so the location names its file by its absolute path, which `{root}` stands
for with forward slashes. Every status comes from the `selftest` family's
vocabulary.

`interface` pairs the installed Python and R interface and handle lifetime
suites, and `yamaa-boundary`'s handshake and census families read it too. Its
outcomes were written from the check order of `rust/interface/README.md`
("Version and feature handshake") before the engine ran: a request for another
version is a mismatch whatever else it holds, a malformed version field leaves
`requested_version` null, missing identifiers come in request order, and an
accepted record enables the required identifiers and then the supported
optional ones, in request order, of a shipped build, which supports every
format. Its message templates are those of the `handshake` section of
`rust/interface/bindings.yaml`. A request only one host can build, such as
R's `NA` or a Python float, is written in that host's suite. The lifetime
study's two output rows were written by hand from its input.

`compact-yaml`, `longitudinal-capacity` and `column-ceilings` pair the
installed Python and R suites of the same names. Where a case is too wide to
write cell by cell (5,000,000 cells, or 1,024 columns), the family holds the
authored rule its cells follow, and each host expands that rule itself; a
host never derives an expected cell from what it built. The longitudinal
values stay below 2^53, so R's plain doubles hold them exactly. Each host
writes its own specification text and inputs and keeps its own route
mechanics: Python reaches a listed specification through `project.spec()`
where it has a domain, R through the project's own `check()` and `run()`.
Without arrow, R skips the Parquet sources and only requires a saved Parquet
output to exist.
The profile's `partitions` is the record each run's measured observations
give under the hosts' default partitions (#3794), key-bounded over two
leading keys with at most 624,640 records each: the profile declares one key,
so its partitions keep one, and its 50,000 records fit in one partition.

`partitioned-default` is the Python and R suites of the same name, which run
only under the supplemental inventory of native qualification: a whole-table
run refuses its 10,000,001 records, and the hosts' default partitions, over
the two leading keys STUDYID and USUBJID that the window groups by, run it a
subject at a time. Its lines were written by hand from the rule: each
subject's ten records number 1 to 10 in SEQ order, and the last record is a
subject of its own.

The R grammar suite's conditional failure and original
CSV preservation controls also remain. Exact float bit witnesses, NUL/invalid
UTF-8 refusals, native ownership, callback order/identity, interrupts, provenance,
publication retries and argument/class assertions stay in each actual host.

`sas-cases.json` adds paired SAS source controls with explicitly authored typed
rows, exact CSV bytes, encoding declarations and refusal causes. Integer
boundaries remain decimal strings. Both hosts consume the canonical transport
and SAS binary inputs under `rust/crates/yamaa-adapters/tests/fixtures/sas`,
staged by the supplemental catalog into `host-fixtures/sas`. Their independent
writer, upstream source notes and fixture licenses stay beside those binaries
in Rust source packages. Compression controls compare every input cell with the
unchanged independent pandas CSV owner. The haven temporal owner asserts its
documented first row only; other rows are never generated as golden truth by
the candidate reader. Each installed host also verifies failed saves and held
output after the original source and specification are changed or removed.

The `original-specifications.typed_intermediates` cases additionally share
authored conversion inputs, expected CSV bytes, refusal conditions and reached
handler counts with the Rust adapter execution controls. Python and R exercise
the same typed bindings through their installed public entry points.
The `original-specifications.temporal_arithmetic` vectors likewise share
PK relative times, civil shifts, duration parsing and writing, static
refusals, range failures and reached `invalid` handler counts across Rust,
Python and R.
The `original-specifications.inheritance_nulls` cases share authored layers,
missing-lookup CSV bytes, reached absence counts and refusal conditions for
missing answers written as YAML null, empty children, `clear:` entries and
rename-only intermediates in both hosts. Each case names `absent_count`,
the number of times it reaches the handler an intermediate's absence counts
under (D202).
The `original-specifications.text_positions` cases share the pieces and
lengths of `str_substr` and `str_length` over ASCII, non-ASCII and empty text
(TEXT-13, TEXT-14), a continuation row filtered on a text's length, and two
static refusals across Rust, Python and R. Each case names its complete issue
rows, or its saved CSV bytes, and `missing_count`, the number of `missing`
answers it reaches, a row template's filtered records included (ROW-3).

## Conditions no case reaches

`unreached-conditions.yaml` lists each condition that `yaml/conditions.yaml`
registers and no parity case reaches (#2846): no negative benchmark's
`expected/error.yaml` names it, and no case of the JSON files here does. Each
entry is unreachable by design, with the reason no case can reach it, or not
covered yet, under the family issue that adds its case.
`tools/validation/check_diagnostics.py` keeps the list exact: it fails on a
registered condition with neither a case nor an entry, and on an entry for a
condition that a case reaches or that is not registered. The hosts never read
the list, so the supplemental catalog stages it nowhere.

## The command line

The command line is the third host of the issue and run-report parity truth
(#2846). `rust/crates/yamaa-cli/tests/parity.rs` runs each case of
`diagnostic-parity` and `project-parity`, and the invalid files of
`parquet-containment`, through the `yamaa` binary with `--format json`. It
compares every issue that the printed document holds with the case's rows:
all twelve columns, in report order, with the context as the canonical JSON
text that the binary printed. It gives each specification as `spec.yaml`,
relative to the case's directory, so `{path}` stands for that. Of the run
reports, it reads the value-level rows of `public-contract.value_level_report`
from the report that the case's specification declares, which the hosts read
from `run.report`, and compares the report that the failed run of
`public-contract` saves with `failed_report_csv`, byte for byte. Those rows
and bytes hold all fourteen report columns, in order. It lints the `lint`
family's partial restatement with and without `--fix` and compares each
finding the document prints with the case's rows, all six lint columns. It
makes the call of each `issue-locations` case, selecting a project's
specification by name as `check` with `--spec`, and compares the first issue
the document prints with the case's condition, first spec path, file, line
and column.
`cargo test` builds and runs the binary, so the leg runs in `make test-rust`,
CI's `rust` job, and in native qualification's workspace tests on each
platform.

A case whose rows or files differ by host gives them by host, under `python`,
`r` and `cli`, and each host reads its own: a case that gives them by host
gives the command line's too. The command line runs no project function and
checks no lock (D110, D123). So `a_function_the_running_host_does_not_implement`
writes it a function file that implements both languages, and both
project-function cases give its `unsupported_operation`, which names the
operation `environment` and is located at the first derivation that calls a
project function (D221). A project run prints any project finding, then
every member's issues in run order, and counts each member's in `members`,
so a case that names a member compares that member's issues.

## Public conformance mapping

The public-conformance plan reads benchmark metadata for membership and
explicit exclusions. Its runtime observations, saved outputs and independent
golden comparison remain the complete public benchmark gate. The following
supplemental controls reuse those same independently authored source/report
owners while adding host or private lifecycle assertions:

- `original-specifications.original_case_names` names active public cases,
  including `schema-lookup`, `schema-inheritance`, `schema-window-functions`,
  `adam-adlb-ordered-sum` and the original negative specification families.
- `public-environment.original_case_names` names the active `schema-functions`,
  `schema-non-finite`, `adam-adsl-bmi` and `adam-advs-percentiles` cases.
- File-resource controls reuse `schema-lookup`; verification-log controls reuse
  `schema-verification-log`; their independent report/CSV truth stays under
  `specification-original`. They also exercise real source deletion, held views,
  native capture authority and repeated publication.
- Define controls continue to use `define-fixtures` and its independent schemas,
  JSON/XML/CSV owners. Grammar decisions continue to use `grammar-original` and
  its independently authored case table.

Unpaired scalar and project-function controls retain their existing owners.

## Staging and qualification

The supplemental catalog declares both readers, the shared JSON owner and the
ODM reader documents. `tools.commands.installed` stages their exact bytes
outside the checkout for both hosts, refuses overwrites, and the installed
runner checks them before starting any suite. The standalone R source package carries the same files in
`tests/host-fixtures`, so `R CMD check` and separately staged installed tests use
the same owned truth. The readers select the staged files beside the suites,
with no checkout fallback. The R suites find them in their working directory:
the copy of the package's tests directory that `R CMD check` runs them in, or
the directory where native qualification stages them. R does not install them.

The ordinary lanes run every suite that reads this truth, so a parity
regression fails CI's `python` and `r` jobs on the exact head, not only the
`equivalence` job. `make test-python` stages every
`python/tests/installed/installed_*.py` with these files, runs each in its own
process beside pytest, and prints each suite's name and result
(`== installed_<suite>.py`). `make test-r` runs `R CMD check` on the staged
package, whose `tests/` holds every `R/yamaa/tests/*.R` and this directory,
and its log names each suite as it runs (`Running '<suite>.R'`). Every family
of `cases.json` has a suite in each host, and the equivalence gate runs the
same suites on fresh installs. A suite decodes `cases.json` once, however many
families it reads.

The R codec uses jsonlite from the package's `Suggests`. The native
workflow installs it before supplemental R tests; production R does not import
it. A suggested package may be missing where the package is checked, so each
R suite skips, with a message, what reads this truth when jsonlite is not
installed. Successful local codec/source checks do not qualify an installed R
runtime.
Release qualification requires the complete Python/R platform matrix, installed
suites and public conformance on the final revision.
