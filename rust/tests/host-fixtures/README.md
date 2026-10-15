# Installed host semantic fixtures

`cases.json` owns independently authored inputs and expected truth repeated by
installed Python and R controls. The two readers only decode this data. They do
not invoke a native engine, reference evaluator, benchmark or callback to make
expected values. Integer boundaries remain decimal strings; each host constructs
its original integer type. Text is UTF-8, and CSV comparisons retain all bytes,
including quoting, trailing zeros and final newlines.

The R assertions share `context.R` to compare decoded diagnostic objects with
key order ignored at every level. Array order and scalar types remain exact.

The private and public producer suites share the `producer-graph` section. The
function contract has an explicit `HOST_CALLABLE` slot because the installed
Python and R programs have different callable names. Environment languages,
locks, ABI handles and callbacks remain host-owned. Public-contract data retains
the different Python and R i64 vectors, including R's value beyond 2^53.

| Paired family | Shared data and existing owners |
| --- | --- |
| producer-builds | Producer/consumer YAML, function cases, exact rounded CSV, aliases and graph variants |
| public-producer-builds | Same producer graph, retained-source names, exact precision CSV, refusal variants |
| producer-cohort | `project-original/shared-producer-parquet`, source filenames and exact output CSV |
| original-specifications | Original case selection, lookup/ordering/casing inputs and exact CSV; existing independent TSV/JSON reports |
| file-resources | Named lookup paths, issue schema and exact alias context; existing inspection/capture reports |
| public-environment | Original case selection, graph references, quota/version inputs and exact callback CSV |
| verification-logs | `specification-original/schema-verification-log`, artifact names, evaluated counts and fatal outcomes |
| producer-logs | Decimal i64 keys, aliased/diamond inputs and complete verification/warning CSV goldens |
| public-define | Existing Define input/XML/JSON owners, creation time, column schemas and numeric metadata variants |
| public-contract | Shared YAML, issue/argument/view names, empty/fatal log variants; distinct host i64 vectors |
| public-grammar | Shared condition families; existing `grammar-original/cases.tsv` decisions |

The R grammar suite's conditional failure and original
CSV preservation controls also remain. Exact float bit witnesses, NUL/invalid
UTF-8 refusals, native ownership, callback order/identity, interrupts, provenance,
publication retries and argument/class assertions stay in each actual host.
No paired suite is replaced by a reference result or removed from the catalog.

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
PK relative times, civil shifts, duration parsing/formatting, static refusals,
range failures and reached `invalid` handler counts across Rust, Python and R.
The `original-specifications.inheritance_nulls` cases share authored layers,
missing-lookup CSV bytes, reached `no_match` counts and refusal conditions for
introducing null values, empty children and inherited clears in both hosts.

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
- `shared-producer-parquet` is an additional installed ownership/publication
  control, rather than an active public-conformance benchmark name.

Unpaired scalar, inheritance, project-function, Arrow and ODM helpers retain
their existing ownership and installed suite identities.

## Staging and qualification

The supplemental catalog declares both readers and the shared JSON owner.
`tools.commands.installed` stages their exact bytes outside the checkout for both
hosts, refuses overwrites, and the installed runner checks them before starting
any suite. The standalone R source package carries the same files in
`inst/host-fixtures`, so `R CMD check` and separately staged installed tests use
the same owned truth. The readers select the staged files or installed package
resources, with no checkout fallback.

The R codec uses jsonlite from the package's `Suggests`. The native
workflow installs it before supplemental R tests; production R does not import
it. Successful local codec/source checks do not qualify an installed R runtime.
Release qualification requires the complete Python/R platform matrix, installed
suites and public conformance on the final revision.
