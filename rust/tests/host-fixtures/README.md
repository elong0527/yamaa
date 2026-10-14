# Installed host semantic fixtures

`cases.json` owns independently authored inputs and expected truth repeated by
installed Python and R controls. The two readers only decode this data. They do
not invoke a native engine, reference evaluator, benchmark or callback to make
expected values. Integer boundaries remain decimal strings; each host constructs
its original integer type. Text is UTF-8, and CSV comparisons retain all bytes,
including quoting, trailing zeros and final newlines.

The private and public producer suites share the `producer-graph` section. The
function contract has an explicit `HOST_CALLABLE` slot because the installed
Python and R programs have different callable names. Environment languages,
locks, ABI handles and callbacks remain host-owned. Public-contract data retains
the different Python and R i64 vectors, including R's value beyond 2^53.

| Paired family | Shared data and existing owners | Python assertion sites | R assertion sites |
| --- | --- | ---: | ---: |
| producer-builds | Producer/consumer YAML, function cases, exact rounded CSV, aliases and graph variants | 95 | 30 |
| public-producer-builds | Same producer graph, retained-source names, exact precision CSV, refusal variants | 171 | 44 |
| producer-cohort | `project-original/shared-producer-parquet`, source filenames and exact output CSV | 12 | 9 |
| original-specifications | Original case selection, lookup/ordering/casing inputs and exact CSV; existing independent TSV/JSON reports | 637 | 421 |
| file-resources | Named lookup paths, issue schema and exact alias context; existing inspection/capture reports | 39 | 17 |
| public-environment | Original case selection, graph references, quota/version inputs and exact callback CSV | 157 | 52 |
| verification-logs | `specification-original/schema-verification-log`, artifact names, evaluated counts and fatal outcomes | 53 | 22 |
| producer-logs | Decimal i64 keys, aliased/diamond inputs and complete verification/warning CSV goldens | 85 | 43 |
| public-define | Existing Define input/XML/JSON owners, creation time, column schemas and numeric metadata variants | 144 | 56 |
| public-contract | Shared YAML, issue/argument/view names, empty/fatal log variants; distinct host i64 vectors | 70 | 30 |
| public-grammar | Shared condition families; existing `grammar-original/cases.tsv` decisions | 2 | 4 |

All 186 Python test methods, 1,465 Python assertion sites and 728 R assertion
sites remain in the named paired suites above. Source controls and decoded
independent literals were checked before and after consolidation.
The R grammar suite's conditional failure and legacy
CSV preservation controls also remain. Exact float bit witnesses, NUL/invalid
UTF-8 refusals, native ownership, callback order/identity, interrupts, provenance,
publication retries and argument/class assertions stay in each actual host.
No paired suite is replaced by a reference result or removed from the catalog.

## Public conformance mapping

The existing public-conformance plan owns 318 authored benchmark names, including
316 active cases. Its runtime observations, saved outputs and independent golden
comparison remain the complete public benchmark gate. The following supplemental
controls reuse those same independently authored source/report owners while
adding host or private lifecycle assertions:

- `original-specifications.original_case_names` names 17 active public cases,
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
`installed.py stage` copies their exact bytes outside the checkout for both
hosts, refuses overwrites, and the installed runner checks them before starting
any suite. The standalone R source package carries the same files in
`inst/host-fixtures`, so `R CMD check` and separately staged installed tests use
the same owned truth. The readers select the staged files or installed package
resources, with no checkout fallback.

The R codec is the already pinned jsonlite 2.0.0 test dependency. The native
workflow installs it before supplemental R tests; production R does not import
it. Successful local codec/source checks do not qualify an installed R runtime.
The reviewed combined source must pass the actual full Python/R platform matrix,
including installed suites, public conformance and the existing release audit.
