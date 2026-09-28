# SDTM to ADaM derivation recipe

Canonical recipe for deriving ADaM datasets from SDTM with the yamaa engine.

- create exactly one spec per output dataset (e.g. `adsl.yaml`); keep temporary
  values and ordered calculation steps as derivations in that spec, not as
  `adsl-s1.yaml`, `adsl-final.yaml`, or other staged dataset files
- establish dependency to avoid duplicate logic (e.g. the rest of the ADaM specs should depend on adsl.yaml)
- create `run.py` to run the pipeline within 30 lines of code using the yamaa Python engine.
- keep study-owned orchestration beside the study; do not add renderers,
  conversion utilities, or other study support files under `.github/scripts`
- verify equivalence of all data with a tolerance at 1e-10.
- Equivalence means 100% of columns and 100% of cells match, with zero validation issues.
- Comparison lives in a separate `compare.py` (not in `run.py`), which reports matched/total columns and cells per dataset and exits nonzero on any mismatch.
- Always verify the derivation against the latest yamaa repo before declaring equivalence.
  
The original pilot source code may be read to understand intent, but
the yamaa spec must be built as a robust and succinct yamaa spec.

## 1. Prerequisites

- **Input** The project's `data/sdtm/` holds the staged SDTM
  inputs as parquet, one file per domain (e.g. `dm.parquet`, `ae.parquet`).
- **yamaa** learn yamaa structure from project github README file. 

## 2. Spec authoring conventions

- **Uniform derivations belong at column level.** A derivation identical
  across every row template (group-key echoes like `STUDYID`/`USUBJID`,
  literals like `DOMAIN`, `PARAMCD` assignments) goes in
  `columns[].derivation`, never inside `rows:` derivations. Row templates
  should be minimized for section level details. 
- **Declaration order is load-bearing.** A column must be declared after any
  column its derivation references.
- **Ordered window chains stay in the dataset spec.** A later window derivation
  may read an earlier window result, including through intervening scalar
  derivations. For example, `rank` -> `case` -> `previous_non_missing` is one
  ordered derivation chain in one row template; it does not require staging
  files.
- **ADSL is derived once and consumed downstream.** Subject-level variables
  are derived only in the ADSL spec; the other specs read them from the
  derived `adsl-yamaa.parquet` predecessor.
- **Every column declares `label:`** 
- **Output naming.** Every spec's `output.path` is `adam/<ds>-yamaa.parquet`
  (e.g. `adam/adsl-yamaa.parquet`).

## 3. Derivation order and dependency handling

Derive in dependency order; every predecessor is staged before it is read:

1. **Staging specs** (e.g. per-record exposure staging) - pure SDTM input.
2. **ADSL** - from SDTM plus staging specs.
3. **Other ADaM** - each from SDTM, the derived
   `adsl-yamaa.parquet`, and any derived predecessors it needs (e.g. ADTTE reads
   the derived ADAE; ADLBC reads the derived ADSL).

## 4. When one specification is blocked

Do not work around a missing language or runtime capability by committing
multiple YAML files for one output dataset. Instead:

1. Reduce the blocker to the smallest single-spec example and verify it against
   the latest yamaa revision.
2. If it still fails, create a yamaa GitHub issue with the `pilot-dry-run`
   label. Include the reduced specification, the diagnostic or timeout, and
   the required before/after behavior.
3. Leave the output dataset out of the study run until that issue is resolved;
   do not commit its staged workaround as the intended derivation.
