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
installed Python and R controls. The two readers only decode this data. They do
not invoke a native engine, reference evaluator, benchmark or callback to make
expected values. Integer boundaries remain decimal strings; each host constructs
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
| original-specifications | Original case selection, lookup/ordering/casing inputs and exact CSV; existing independent TSV/JSON reports |
| file-resources | Named lookup paths, issue columns, exact alias context and entry-relative programs that declare `output.report`; existing inspection/capture reports |
| public-environment | Original case selection, schema link references, quota/version inputs, exact callback CSV and the project-relative inherited and codelist files |
| verification-logs | `specification-original/schema-verification-log`, evaluated counts and fatal outcomes, now checked in the run report |
| public-define | Existing Define input/XML/JSON owners, creation time, issue columns, column schemas, numeric metadata variants, spec/project text mutations and the issue row of a lower-cased transport name collision (its context compared by the file name both writers would write) |
| public-contract | Shared YAML, issue and report columns, argument/member/read-only view names and defaults, shared messages (D39's error text, written from `docs/design/interface-1.0/api.md` and the templates of `yaml/conditions.yaml`; `{path}` stands for the specification or project path the test gave, and `{file}` for the file the first issue is located in, as its location names it), typed plain values, the failed run's report bytes, the issue rows of a save with nothing to write, and the value-level check rows of a run whose entry selects no row (SUB-13); distinct host i64 vectors |
| public-grammar | Shared condition families; existing `grammar-original/cases.tsv` decisions |
| diagnostic-parity | Refusal cases with their complete expected issue rows; `{path}` in a row's message and context stands for the specification path the test gave |
| read-odm | The two reader documents (the adapters' `odm` fixtures), all 30 columns and cells, archive member selection, the authored sources two refusals read, and each refusal's one issue row and error text (`{source}` stands for the source path the test gave) |
| parquet-containment | A Parquet file that makes the native decoder panic, its issue row, and a readable file with its saved CSV |
| plan-reuse | One specification run A, B, A on one object with exact output CSV and issue rows; a changed specification's output; a changed function default's output |
| lint | One specification with layout, authoring and suppression findings: its exact `lint()` rows, the text the proved fixes write, the rows that remain, and two refusals |
| selftest | The bundled cases in list order, the cases table's columns, the status vocabulary, and the report's lines and the layout of its information line |
| schema | The four file types, each JSON Schema's dialect, title and `$ref`, the canonical order of its top-level keys, and two refusals with their error text and issue rows |
| info | The engine information's fields in order, the host vocabulary, and a shipped build's language versions and features |
| printing | A study's files, and the status and printed lines of each result: a check, a failed check, a project check, a run, a failed run, a project run and each of its members, a submission, and a lazy specification and project; `{path}` stands for the path the test gave and `{root}` for its study directory, with forward slashes |
| issue-locations | One case for each rule of P-ISSUE-1 (`docs/design/interface-1.0/api.md`, "Issue locations"): the files a case writes, its call, and its first issue's condition, first spec path, file, line and column, counted by hand in those files |
| project-parity | Project cases with their complete expected issue rows: the files a case writes, its call (a member of a project run, a specification the project names, or a submission) and every issue row; rows and files that depend on the host language are given by language, `{root}` stands for the case's directory and `{version}` for the installed yamaa version |
| compact-yaml | A specification with a block and a flow input, the block, compact, flow and explicit-key verification forms with their one output and saved CSV, failed row-count bounds, malformed closers, and raw NULs with their `invalid_yaml` context (#2196, #3125) |
| longitudinal-capacity | The 50,000-row, 100-column profile: its column names, its value rule (the row index for the key, then an offset plus the row and column index modulo a prime), the size its CSV must exceed, and its source and output formats |
| column-ceilings | The 1,024 admitted and 1,025 refused widths of each boundary, the compiler-limit, parent-occurrence and wide-integer issue rows (`{digits}` stands for the wide integer), the repaired capacity's remaining error, the inheritance depth that builds and the wide inherited cases' rows by column index modulo 3 |

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
suite replaces, as JSON text in the context. Together the cases cover every
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
need a project: a member of a project run, whose `unlinked_producer` and
`producer_not_completed` findings name the listed specification they belong
to and the document they come from (PROJECT-7), the run of a specification
the project names, and a submission, whose `duplicate_dataset_entry`
refuses a section that lists two specifications of one saved artifact
(DEFINE-5). A case whose files or rows depend on the host language gives
them by language: a function file that implements only the other host
refuses the call with the running host's language (FUNC-1), and each host
verifies only its own lock (FUNC-3). The function files name
standard-library callables (`operator.pos`, `base::identity`), which no lock
pins and which the R check environment has, so neither host installs a
project package for these cases. `{root}` stands for the case's directory,
resolved, with forward slashes, and `{version}` for the installed yamaa
version. The rows were written from those cards, the message templates of
`yaml/conditions.yaml` and the issue locations of P-ISSUE-1 before the
engine ran.

`read-odm` pairs the installed Python and R `read_odm` suites. Its documents
are the reader suite's own `odm13.xml` and `odm20.xml`, staged from
`rust/crates/yamaa-adapters/tests/fixtures/odm` as `host-fixtures/odm`; the
expected cells were read from the documents by hand. `SourceOrdinal` is
decimal text in the truth (an integer in Python, a double in R) and every
other cell is text or null. Each refusal holds the one issue row that
`yamaa.Error` or `yamaa_error` carries, and its error text is D39's prefix
and that issue; `{source}` stands for the source path the test gave. Its
`files` are sources that are neither document nor archive, which both
suites write beside the documents: a plain document under a `.tar` name,
which ODM-1 refuses as an archive name holding no archive, and a bzip2
stream, refused from its first bytes before anything is decompressed.

`parquet-containment` pairs the installed Python and R containment suites.
Both files were authored field by field. In the first, the column chunk of an
INT96 column holds a 13-byte `min_value` statistic, which parquet 55.2.0's
metadata decoder asserts is 12 bytes long. The engine contains that panic as
one `source_parquet_invalid` issue, and the adapters' reader test checks that
these bytes still reach the panic. The second file holds the INT64 values 1
and 2, which a later run in the same process reads and saves. Each suite also
runs both specifications in a fresh process and requires its stderr to be
empty, so the R suite proves containment on the Windows GNU build too.

`selftest` pairs the installed Python and R selftest suites. Its cases are
the list in `benchmarks/selftest.yaml`, and its lines are the printed lines of
#3030 D119 for a shipped build, which compiles every feature, so every case
passes, and a listed case covers every adapter, so no `not covered:` line is
printed. Each host fills the information line from its own facts: the
compiled core version, its interpreter's version, and the platform its
interpreter reports.

`schema` pairs the installed Python and R `schema()` suites (#3030 D149). Its
facts were read by hand from the JSON Schemas committed in `yaml/v1/json/`,
and its refusal text was written from D39's prefix and the template that
`yaml/conditions.yaml` holds for `unexpected_file_type`. The suites check that
each schema is one ASCII line whose top-level keys come in key order, as
canonical JSON text keeps them; the facade's tests compare the whole text
with the committed files.

`info` pairs the installed Python and R `info()` suites (#3030 D41). Its
fields are those of `records.info` in `rust/interface/bindings.yaml`, and its
features and hosts that record's vocabularies, which a tools test checks. The
`selftest` suites read it for a selftest's `info`, which `info()` equals.

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
PK relative times, civil shifts, duration parsing, static refusals,
range failures and reached `invalid` handler counts across Rust, Python and R.
The `original-specifications.inheritance_nulls` cases share authored layers,
missing-lookup CSV bytes, reached `no_match` counts and refusal conditions for
`{literal: null}` handlers, nulls that leave a field absent, empty children and
inherited clears in both hosts.

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

The R codec uses jsonlite from the package's `Suggests`. The native
workflow installs it before supplemental R tests; production R does not import
it. A suggested package may be missing where the package is checked, so each
R suite skips, with a message, what reads this truth when jsonlite is not
installed. Successful local codec/source checks do not qualify an installed R
runtime.
Release qualification requires the complete Python/R platform matrix, installed
suites and public conformance on the final revision.
