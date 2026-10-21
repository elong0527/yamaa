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
| public-define | Existing Define input/XML/JSON owners, creation time, issue columns, column schemas, numeric metadata variants and spec/project text mutations |
| public-contract | Shared YAML, issue and report columns, argument/member/read-only view names and defaults, shared messages (D39's error text, written from `docs/design/interface-1.0/api.md` and the templates of `yaml/conditions.yaml`; `{path}` stands for the specification or project path the test gave), typed plain values and the failed run's report bytes; distinct host i64 vectors |
| public-grammar | Shared condition families; existing `grammar-original/cases.tsv` decisions |
| diagnostic-parity | Refusal cases with their complete expected issue rows |
| read-odm | The two reader documents (the adapters' `odm` fixtures), all 30 columns and cells, archive member selection, and each refusal's one issue row and error text (`{source}` stands for the source path the test gave) |
| parquet-containment | A Parquet file that makes the native decoder panic, its issue row, and a readable file with its saved CSV |
| plan-reuse | One specification run A, B, A on one object with exact output CSV and issue rows; a changed specification's output; a changed function default's output |
| lint | One specification with layout, authoring and suppression findings: its exact `lint()` rows, the text the proved fixes write, the rows that remain, and two refusals |
| selftest | The bundled cases in list order, the cases table's columns, the status vocabulary, the report's lines and the layout of its information line, the engine information's fields in order, and a shipped build's features and language versions |
| schema | The four file types, each JSON Schema's dialect, title and `$ref`, the canonical order of its top-level keys, and two refusals with their error text and issue rows |

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
no JSON reader chooses a host number type. Together the cases cover every
value shape a context holds: integers at and beyond 2^53, the signed 64-bit
bounds and a wider literal, fractional and whole floats, null, non-ASCII text,
text holding NUL (escaped), nested and empty objects and arrays, absent and
present geometry, an empty route, an absent rule, empty specification
paths and several rows. A zero-width span is not reachable from a
specification, because spans come only from expression nodes; the adapter
record tests own it. Spans are integer pairs, doubles in R, and both geometry
fields are null (`None`, `NULL`) when nothing was recorded.

`read-odm` pairs the installed Python and R `read_odm` suites. Its documents
are the reader suite's own `odm13.xml` and `odm20.xml`, staged from
`rust/crates/yamaa-adapters/tests/fixtures/odm` as `host-fixtures/odm`; the
expected cells were read from the documents by hand. `SourceOrdinal` is
decimal text in the truth (an integer in Python, a double in R) and every
other cell is text or null. Each refusal holds the one issue row that
`yamaa.Error` or `yamaa_error` carries, and its error text is D39's prefix
and that issue; `{source}` stands for the source path the test gave.

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
passes and only the adapter that the list leaves uncovered is named. Each
host fills the information line from its own facts: the compiled core
version, its interpreter's version, and the platform its interpreter reports.

`schema` pairs the installed Python and R `schema()` suites (#3030 D149). Its
facts were read by hand from the JSON Schemas committed in `yaml/v1/json/`,
and its refusal text was written from D39's prefix and the template that
`yaml/conditions.yaml` holds for `unexpected_file_type`. The suites check that
each schema is one ASCII line whose top-level keys come in key order, as
canonical JSON text keeps them; the facade's tests compare the whole text
with the committed files.

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
