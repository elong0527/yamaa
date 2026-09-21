# SDTM to ADaM derivation recipe

Canonical recipe for deriving ADaM datasets from SDTM with the yamaa engine.
Battle-tested on Pilot 1 (CDISCPILOT01 staging), Pilot 3 (CDISCPILOT03: ADSL,
ADAE, ADADAS, ADTTE, ADLBC at 100% cells), and Pilot 5 (CDISCPILOT01:
ADSL, ADAE, ADADAS, ADTTE, ADLBC). Future scheduled jobs must follow this
recipe verbatim; project-specific choices (dataset list, keys, SDTM sources,
engine pin) go in the project folder, not in these instructions.

The original pilot source code (R/SAS) may be read to understand intent, but
the yamaa spec must be built as a robust yamaa spec, never as a
line-by-line transliteration.

## 1. Prerequisites

- **Input staging.** The project's `data/sdtm/` holds the staged SDTM
  inputs as parquet, one file per domain (e.g. `dm.parquet`, `ae.parquet`),
  verified cell-identical to the official sources. The project's
  `data/adam/` holds the reference ADaM parquets with variable labels
  preserved as `yamaa:label` Arrow field metadata (this is the comparison
  target; it is never used as a derivation input).
- **yamaa version pinning.** The engine's schema bundle (`yaml/schema.yaml`)
  lives at the yamaa repo root, outside the Python package, so install the
  engine **editable from git** pinned to the merge commit the specs were
  verified against, plus `polars` and `pyarrow`:

  ```text
  -e git+https://github.com/elong0527/yamaa.git@<merge-sha>#egg=yamaa&subdirectory=python
  polars
  pyarrow
  ```

  A plain wheel install cannot find the schema bundle. Record the pin in
  `requirements.txt` and re-verify it whenever main moves.

## 2. Spec authoring conventions

- **Uniform derivations belong at column level.** A derivation identical
  across every row template (group-key echoes like `STUDYID`/`USUBJID`,
  literals like `DOMAIN`, `PARAMCD` assignments) goes in
  `columns[].derivation`, never inside `rows:` derivations. Row templates
  keep only template-specific logic: group reductions and case-split
  derivations.
- **Declaration order is load-bearing.** A column must be declared after any
  column its derivation references (the validator fails forward references;
  the planner executes in stable dependency order). When reordering, keep
  this invariant.
- **ADSL is derived once and consumed downstream.** Subject-level variables
  are derived only in the ADSL spec; the other specs read them from the
  derived `adsl-yamaa.parquet` predecessor. Do not re-derive them and do not
  duplicate intermediate derivations across specs (e.g. exposure staging is
  consumed from a staging spec, never re-scanned from raw EX).
- **Every column declares `label:`** sourced from the reference ADaM's
  `yamaa:label` metadata, cross-checked against define-adam.xml when
  present. The engine's parquet profile carries no field metadata of its
  own, so the pipeline stamps labels onto the derived parquet as
  `yamaa:label` after derivation.
- **Missing-value convention.** The reference ADaM was produced by R, where
  a missing character value is `""`; yamaa reads missing input strings as
  null. For character columns the reference leaves blank, derive with
  `first_available(..., missing: "")` so outputs carry `""`, not null.
- **Output naming.** Every spec's `output.path` is `<ds>-yamaa.parquet`
  (lowercase domain, e.g. `adsl-yamaa.parquet`).
- **Input paths are relative to the spec file.** Specs reference SDTM as
  `sdtm/<dom>.parquet` and derived predecessors as
  `adam/<ds>-yamaa.parquet`; `run.py` stages exactly this layout into the
  work directory. Commit only what the spec cannot generate itself:
  hand-built planning relations (e.g. LOCF plans, analysis-window lookups)
  go in as CSV (the repo structure check forbids `.parquet` outside
  `data/`), and `run.py` stages them to parquet with pinned dtypes at run
  time. If the spec needs a derived column the planner's static gate cannot
  yet produce (e.g. parsing ISO date text), derive it in a staging step
  inside `run.py` from the staged inputs, never by hand-editing data.

## 3. Derivation order and dependency handling

Derive in dependency order; every predecessor is staged before it is read:

1. **Staging specs** (e.g. per-record exposure staging) - pure SDTM input.
2. **ADSL** - from SDTM plus staging specs.
3. **ADAE / ADADAS / ADTTE / ADLBC** - each from SDTM, the derived
   `adsl-yamaa.parquet`, and any derived predecessors it needs (ADTTE reads
   the derived ADAE; ADLBC reads the derived ADSL). `run.py` auto-includes
   predecessors when a subset is requested.

Derived predecessors are staged into the work directory exactly where the
specs' input paths point, so the whole pipeline is self-regenerating from
SDTM - the reference ADaM is never an input.

## 4. run.py contract (strict)

`run.py` derives datasets. Nothing else.

**Must do:**

- Stage the SDTM parquets and planning inputs into a `work/` directory.
- Run each spec in dependency order via exactly:

  ```python
  import yamaa

  run = yamaa.yamaa_domain("<ds>.yaml")
  frame = run.output          # the derived dataset
  out = run.save()            # persists <ds>-yamaa.parquet
  ```

- Fail fast on any non-empty validation diagnostics (`run.issues`): print
  them and exit nonzero. A dataset with diagnostics is not a dataset.
- Write the five (or however many the project declares) `*-yamaa.parquet`
  files with variable labels to `work/derived/`. `work/` is git-ignored
  build output and is never committed.

**Must NOT do:**

- Compare against the reference ADaM. That is `compare.py`'s job alone.
- Derive any ADaM variable in Python. The only Python allowed is reusable
  extension helpers (e.g. a BMI function) that the spec calls; never scripts
  that directly derive variables.
- Hand-edit, patch, or post-process derived outputs. What the spec
  produces is what ships to comparison.

## 5. compare.py contract

`compare.py` is a standalone script that verifies the derived datasets
against the reference ADaM and reports, per dataset, matched/total columns
and matched/total cells. It derives nothing.

Comparison semantics:

- Rows align on the dataset keys (unique in both derived and reference;
  zero unmatched rows on either side, and key columns count as matched).
- Numeric cells match when `|derived - reference| <= 1e-10` (absolute);
  NaN is normalized to null first, and exactly-one-null is a mismatch.
- Non-numeric cells match exactly, with null and `""` normalized (the
  reference was produced by R, which writes `""` for missing).
- Every column carrying the reference's `yamaa:label` metadata must carry
  the identical label in the derived parquet; unlabeled derived columns are
  mismatches.
- Reference columns the spec does not derive are reported as uncovered;
  derived-only columns are reported, never compared.
- Exit nonzero on any mismatch.

Expose the core logic as an importable helper (e.g.
`compare(output, reference, name)`) so individual datasets can be checked
interactively.

## 6. Verification bar

The pipeline is done only when **all** of the following hold:

- 100% matched columns and 100% matched cells on every dataset.
- Zero validation diagnostics on every spec.
- The full Python test suite passes and `ruff check` / `ruff format --check`
  are clean (on the yamaa side, when spec work touched it).
- `run.py` completes end-to-end from a fresh clone with only
  `pip install -r requirements.txt` + `python3 run.py`.

## 7. Failure protocol

When a dataset does not reach 100%:

1. Characterize the gap precisely: which columns, how many cells, and the
   mechanism (e.g. reference rows the spec cannot produce, label
   differences, padding, null-vs-`""`).
2. First suspect the spec, not the engine. Fix spec-side gaps in the spec
   (row templates, missing conventions, value mappings).
3. If the gap is a genuine schema or engine limitation, file **one**
   concise, publicly verified issue in elong0527/yamaa per gap (one
   paragraph plus a before/after spec change). Never file duplicate or
   fabricated issues.
4. Never hand-edit derived outputs to make comparison pass. The comparison
   must reflect what the spec alone produces.

No pull request is opened until the bar in section 6 holds; nothing merges
without the project owner's explicit pick.
