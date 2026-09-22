# SDTM to ADaM derivation recipe

Canonical recipe for deriving ADaM datasets from SDTM with the yamaa engine.

- create one spec per dataset (e.g. adsl.yaml)
- establish dependency to avoid duplicate logic (e.g. the rest of the ADaM specs should depend on adsl.yaml)
- create `run.py` to run the pipeline within 30 lines of code using the yamaa Python engine.
- verify equivalence of all data with a tolerance at 1e-10.
- Equivalence means 100% of columns and 100% of cells match, with zero validation issues.
- Comparison lives in a separate `compare.py` (not in `run.py`), which reports matched/total columns and cells per dataset and exits nonzero on any mismatch.
- Always verify the derivation against the latest yamaa repo before declaring equivalence.
  
The original pilot source code may be read to understand intent, but
the yamaa spec must be built as a robust and succinct yamaa spec.

## 1. Prerequisites

- **Input** The project's `data/sdtm/` holds the staged SDTM
  inputs as parquet, one file per domain (e.g. `dm.parquet`, `ae.parquet`).
- **yamaa** The engine's schema bundle (`yaml/schema.yaml`)
  lives at the yamaa repo root. Leverage `benchmark/` to learn best practice. 

## 2. Spec authoring conventions

- **Uniform derivations belong at column level.** A derivation identical
  across every row template (group-key echoes like `STUDYID`/`USUBJID`,
  literals like `DOMAIN`, `PARAMCD` assignments) goes in
  `columns[].derivation`, never inside `rows:` derivations. Row templates
  should be minimized for section level details. 
- **Declaration order is load-bearing.** A column must be declared after any
  column its derivation references.
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

## 4. File github issues 

 1. explore areas to improve yamaa schema to simplify development
 2. create github issues in yamaa repo with tag: pilot-dry-run 
