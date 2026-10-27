# opensas reference solutions

The `result.sas` programs prepare the prompted derivation benchmarks for
execution with the existing [opensas v0.7.1 release](https://github.com/kirha-ai/opensas/releases/tag/v0.7.1).
Each program reads `/app/input` and writes the CSV files requested by its full
prompt to `/app/output`. The race/ethnicity solution writes both DM and SUPPDM.
Identifiers are read as character values to preserve leading zeros; numeric
measurements and sequence variables use explicit numeric input.

The Harbor opensas track executes supported native CSV cases with the
existing opensas v0.7.1 release. The image installs an unmodified release
executable; these programs do not implement or modify an execution engine. Benchmarks
without derivation prompts, including yamaa specification rejection cases,
are outside this collection.

## Execution prerequisites

Provide a writable `/app/output`, the benchmark's inputs under `/app/input`,
and an independently installed opensas executable. Copy that benchmark's
`result.sas` into the output directory and execute:

```sh
opensas /app/output/result.sas
```

For a local run using other directories, replace the two absolute directory
literals in a temporary copy of the program. Run each benchmark in a separate
process with an empty output directory. Require the expected files and compare
their contents; the reviewed release can return exit zero for a nonexistent
program, so the exit status alone cannot establish success.

`adam-adsl-randomization` requires its `dm.parquet` input to be staged as
`dm.csv` with the same columns, character identifiers, missing values and ISO
date/datetime values. opensas has no native Parquet reader in this release.
Refresh this I/O conversion whenever the inputs change. Do not substitute the
ODM input for DM or use a conversion to perform derivations. The Harbor opensas
selection excludes this case; no converter is included.

## Checks

The current track population is discovered from prompts and each case's
`benchmark.yaml`; excluded cases carry their own reasons. Run original-data
reference checks with an available opensas executable:

```bash
uv run --project python --no-sync pytest evaluations/harbor/tests/test_harbor_opensas.py
```

The tests compare required outputs using the grader's column types, keys
and tolerances. Independent reference-edge tests exercise temporary input
changes. Harbor's oracle preflight additionally runs the actual Linux image,
sandbox and held-out/changed-input verifier:

```bash
uv run --project evaluations/harbor --no-sync python -m evaluations.harbor.smoke \
    --language opensas --all-cases --out /tmp/yamaa-opensas-check
```

The grader's deterministic challenge renames subjects and perturbs supported
ages, measurements and complete dates. Its reduced-subject check recomputes
answers after dropping subjects. These controls do not establish parity with
SAS Institute software.

`schema-adsl-age-quality` also declares a run report. It is a schema
benchmark with no Harbor prompt, and run reports are not graded.

## Compatibility gaps and solution choices

- **Whitespace needs raw records.** opensas's CSV reader and exporter
  normalize the spaces-only comment in `adam-adsl-investigator-comment` to
  character missing. Its reference reads raw records instead, counts each
  field's length with `lengthc(_infile_)`, and writes its CSV with a DATA step,
  so the two-space comment and `CMNTFL=Y` survive the original, held-out and
  changed-input checks. The track still excludes the case. Whether SAS 9.4
  keeps the trailing blanks of `_infile_` the same way is unverified.
- **Blank search strings.** INDEX returns zero for a blank one-character
  excerpt, so the proper-case reference compares the previous character
  with each word separator in an IN list instead of searching a separator
  string.
- **SQL join expressions.** Numeric and padded character keys are calculated
  in DATA steps before joining SUPPLB and the RELREC members. Function calls
  in join predicates were rejected by the release. Questionnaire membership is filtered into tables
  before joining, avoiding unsupported complex join predicates.
- **Self-join column names.** Nadir values use separate historical date/value
  columns, and unscheduled visits use a sorted carry-forward step. Comparisons
  between identically named columns in self-joins produced incorrect results.
- **Character aggregates.** Period dates are aggregated from filtered tables.
  A character CASE expression inside MIN/MAX produced numeric conversion
  diagnostics and empty dates in the initial implementation.
- **Export columns.** PROC EXPORT applies KEEP= and the other input
  dataset options since v0.7.1 (kirha-ai/opensas#20), so a program exports
  `data=result(keep=...)` directly. A final PROC SQL SELECT remains only
  where it sets the requested column order, as it would under SAS 9.4.
- **Numeric text.** Standardized vital-sign text is formatted explicitly.
  BEST32 exposed floating-point artifacts, and field width alone did not give
  the prompt's required significant-digit precision.

Track the findings and future execution prerequisites in
[issue #1875](https://github.com/elong0527/yamaa/issues/1875); the runtime
differences behind the remaining choices are reported upstream in
[kirha-ai/opensas#14](https://github.com/kirha-ai/opensas/issues/14). No expected data
or prompts were changed to accommodate runtime behavior.

## Licensing scope

These original benchmark programs follow the repository's MIT license. The
evaluation Docker image installs the unmodified opensas release executable
with its Apache 2.0 license, the ReadStat MIT copyright/permission notice and
the Zig MIT notice retained at `/usr/local/share/licenses/opensas`. Their
provenance is documented in [runtime notices](../licenses/opensas/README.md).
The earlier upstream provenance, contract and trademark review remains open;
source preparation is not legal clearance for distribution or SAS Institute
reference testing.

SAS and all other SAS Institute Inc. product or service names are registered
trademarks or trademarks of SAS Institute Inc. in the USA and other countries.
This work is not affiliated with or endorsed by SAS Institute Inc.
