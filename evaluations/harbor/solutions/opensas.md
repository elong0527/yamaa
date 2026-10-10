# opensas reference solutions

The `result.sas` programs prepare all 137 prompted derivation benchmarks for
execution with the existing [opensas v0.6.6 release](https://github.com/kirha-ai/opensas/releases/tag/v0.6.6).
Each program reads `/app/input` and writes the CSV files requested by its full
prompt to `/app/output`. The race/ethnicity solution writes both DM and SUPPDM.
Identifiers are read as character values to preserve leading zeros; numeric
measurements and sequence variables use explicit numeric input.

The Harbor opensas track executes the 134 confident native CSV cases with the
existing opensas v0.6.6 release. The image installs an unmodified release
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

## Preparation checks

Local checks on 2026-10-10 used the official macOS arm64 release asset
`sas-v0.6.6-macos`, reporting `opensas v0.6.6`. The release tag's source ref is
`1e8bc655b6a56c58ab20e263e7a2638063834768`. Programs ran from fresh output
directories, and CSV outputs were compared using the existing cell grader's
column types, keys and tolerances.

| Check | Result |
|---|---|
| Source coverage | 137 of 137 full derivation prompts have `result.sas` |
| Original fixture data | 136 match; 1 whitespace mismatch |
| Native CSV input | 135 matching cases |
| Parquet input staged as CSV | 1 matching case, randomization |
| Changed inputs against the existing Python references | 135 of 136 match; the same whitespace mismatch remains |
| Reduced-subject inputs against the Python references | 134 of 134 checked match; 2 single-subject cases skipped |

The changed-input check uses the existing grader's deterministic subject
renaming and supported age, measurement and complete-date perturbations.
The Python references were first checked against their original goldens;
they then computed expected data for the changed inputs. The opensas programs
perform all their own derivations. This local comparison does not exercise
Harbor's sandbox, trajectory audit or Linux image, and it does not establish
parity with SAS Institute software.

The reduced-subject comparison drops every third subject when possible and
recomputes the expected data with the Python reference. Tumor measurements and
event findings each have one subject and were skipped in that comparison.
The spaces-only comment belongs to a dropped subject in this check, so the
passing reduced-subject run does not resolve its original/changed-input failure.

`adam-adsl-age-quality` matches the prompt's requested ADSL data. Its
specification also declares a warning log that is absent from the full prompt;
the current Harbor builder excludes that benchmark because log outputs are
not graded. Its data-only check is included in the 137-case fixture count,
and it has no existing Python reference for the changed-input check.

## Compatibility gaps and solution choices

- **Whitespace remains unresolved.** `adam-adsl-investigator-comment`
  preserves quoted commas, doubled quotes and multiline text, but the
  spaces-only comment is normalized to opensas character missing. The expected
  CSV requires two spaces and `CMNTFL=Y`; the release writes an empty comment
  and `CMNTFL=N`. Keep this case visible as a failure.
- **SQL join expressions.** Numeric and padded character keys are calculated
  in DATA steps before joining SUPPLB. Function calls in join predicates were
  rejected by the release. Questionnaire membership is filtered into tables
  before joining, avoiding unsupported complex join predicates.
- **Self-join column names.** Nadir values use separate historical date/value
  columns, and unscheduled visits use a sorted carry-forward step. Comparisons
  between identically named columns in self-joins produced incorrect results.
- **Character aggregates.** Period dates are aggregated from filtered tables.
  A character CASE expression inside MIN/MAX produced numeric conversion
  diagnostics and empty dates in the initial implementation.
- **Numeric text.** Standardized vital-sign text is formatted explicitly.
  BEST32 exposed floating-point artifacts, and field width alone did not give
  the prompt's required significant-digit precision.

Track the findings and future execution prerequisites in
[issue #1875](https://github.com/elong0527/yamaa/issues/1875). No expected data
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
