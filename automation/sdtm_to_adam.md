# SDTM to ADaM derivation recipe

Canonical recipe for deriving ADaM datasets from SDTM with the yamaa engine.

- create exactly one spec per output dataset (e.g. `adsl.yaml`); keep temporary
  values and ordered calculation steps as derivations in that spec, not as
  `adsl-s1.yaml`, `adsl-final.yaml`, or other staged dataset files.
  Section 5 reconciles this rule with the dependency-order list below.
- establish dependency to avoid duplicate logic (e.g. the rest of the ADaM specs should depend on adsl.yaml)
- keep study-owned orchestration beside the study; do not add renderers,
  conversion utilities, or other study support files under `.github/scripts`
- verify equivalence of all data with a tolerance at 1e-10.
- Equivalence means 100% of columns and 100% of cells match, with zero validation issues.
- Comparison lives in a separate `compare.py` (not in `run.py`), which reports matched/total columns and cells per dataset and exits nonzero on any mismatch.
- Always verify the derivation against the latest yamaa repo before declaring equivalence.

The original pilot source code may be read to understand intent, but
the yamaa spec must be built as a robust and succinct yamaa spec.

The worked example throughout is Pilot 3
(`submission-pilot3` in `RConsortium/submissions-pilot7-synthetic-data`):
five single-output specs deriving ADSL, ADAE, ADADAS, ADTTE, and ADLBC
at 100% columns and cells (2,291,147/2,291,147) on the verified yamaa
revision.

## 1. Study layout

Specs live at the study level; only the Python orchestration lives under
`program/adam/python/`. Planning and lookup tables live with the data so
repository structure checks pass (`.parquet` belongs under `data/`, not
under `program/`). This is the merged Pilot 3 layout; it supersedes the
branch layout with specs under `program/adam/` and tables under
`program/adam/inputs/`, which failed the structure check.

```text
submission-pilot3/
  spec/yamaa/adsl.yaml
  spec/yamaa/adae.yaml
  spec/yamaa/adadas.yaml
  spec/yamaa/adtte.yaml
  spec/yamaa/adlbc.yaml
  program/adam/python/run.py
  program/adam/python/compare.py
  program/adam/python/make_plan.py
  program/adam/python/requirements.txt
  program/adam/README.md
  data/sdtm/*.parquet
  data/mapping/plan.csv
  data/mapping/aw_lookup.csv
  data/adam/*-yamaa.parquet   # ignored build output
```

`run.py` stays within about 30 lines, resolves paths from the working
directory so it must run from `program/adam`, and stages predecessors in
dependency order before downstream specs read them:

```python
here = Path.cwd()
project_root = here.parent.parent  # cd submission-pilot3/program/adam first
specs = [project_root / "spec" / "yamaa" / f"{n}.yaml" for n in
         ("adsl", "adae", "adadas", "adtte", "adlbc")]
for spec in specs:
    yamaa_domain(spec, project_root=project_root).save()
```

Each spec declares SDTM predecessors plus derived predecessors by their
staged build name (e.g. ADAE reads `../../data/adam/adsl-yamaa.parquet`;
ADTTE reads the derived ADSL and ADAE). `make_plan.py` is the reviewed
rule for the rows the spec cannot create (see section 5); `plan.csv` is
its pinned byte-stable output, and `aw_lookup.csv` is the 4-row analysis
window lookup. `requirements.txt` pins the verified yamaa revision as an
editable install plus `polars` and `pyarrow`.

Working-directory-safe run and CI commands (run from `program/adam`):

```bash
cd submission-pilot3/program/adam
pip install -r python/requirements.txt
python3 python/run.py
python3 python/compare.py
```

Drift check for the pinned planning relation (CI fails closed on mismatch,
also run from `program/adam`):

```bash
python3 python/make_plan.py --out /tmp/plan.regen.csv
diff /tmp/plan.regen.csv ../../data/mapping/plan.csv
```

## 2. Spec section order

Write sections in this order after `keys`: `input`, `output`,
`intermediates` when needed, `columns`, then `rows` when needed.

- `base` goes directly after `input`: it names the input dataset whose
  records build output rows when `rows` is absent.
- a root `filter` goes with the row-construction driver: next to `base`
  when `rows` is absent (e.g. ADSL selects `DM.ACTARMCD <> 'Scrnfail'`
  from its `DM` base). It reads only base driver fields before any
  derivation and is mutually exclusive with `rows`.
- `verifications` goes last, after `columns`/`rows`, matching its
  run-last semantics. Column-level `verifications:` stay inline on the
  column; intermediate `verifications:` stay inline on the intermediate.

```yaml
schema_version: "1.0"
domain: ADSL
keys: [STUDYID, USUBJID]

input:
  DM: ../../data/sdtm/dm.parquet
  EX: ../../data/sdtm/ex.parquet

base: DM

filter: "DM.ACTARMCD <> 'Scrnfail'"

output:
  path: ../../data/adam/adsl-yamaa.parquet
  columns: [STUDYID, USUBJID]

intermediates:
  - id: EX_FIRST
    dataset: EX
    key: [STUDYID, USUBJID]
    order_by: [EX.EXSTDTC]
    keep: first
    no_match: null

columns:
  - name: STUDYID
    type: str
    label: Study Identifier
    derivation: DM.STUDYID

  - name: USUBJID
    type: str
    label: Unique Subject Identifier
    derivation: DM.USUBJID
```

Use blank lines between named intermediate entries, between column
entries, and between row entries so long specs can be scanned. Pilot 3
applies this to all five specs (e.g. one blank line per `SEQ_OCC*`
helper and per `PARAMN` mapping item in ADAE/ADLBC).

## 3. Implicit join versus named intermediate

A dataset-qualified scalar read (e.g. `ADSL.TRTSDT`) goes through the
implicit join: one value per current row, matched on the applicable keys
(the output `keys`, in order, that the right-side dataset also carries),
with a miss answering missing. Use it when the intended match is exactly
the shared keys and no filtering, ordering, or reshaping is needed.

```yaml
columns:
  - name: SITEID
    type: str
    label: Study Site Identifier
    derivation: ADSL.SITEID
```

Pilot 3 ADAE and ADLBC read the derived ADSL this way directly
(`ADSL.SITEID`, `ADSL.TRT01A`, `ADSL.TRTSDT`, ...); neither keeps a named
intermediate for that key match. An older runtime made ADLBC carry an
`ADSL1` named intermediate for speed; yamaa #1485 fixed implicit-join
indexing, so the spec drops that workaround and reads `ADSL.*` directly.

Declare a named intermediate when the read needs filtering, selection,
or a different match key: a `filter`/`order_by`/`keep` cohort (e.g. ADSL
`EX_FIRST`, `DS_EOS`, `SV_V1`), an aggregate or window over the dataset,
a `key` that differs from the applicable output keys (e.g. ADADAS
`AW_LOOKUP` matches `AW` on `[AVISIT]`), a `between` window, per-record
`derivations`, or a non-missing enforcement. A rename-only intermediate
(`id` plus `dataset` plus `no_match: null` and nothing else) is rejected:
read the dataset directly instead.

## 4. Column derivation versus row derivation

Put calculations shared across row templates in `columns[].derivation`;
keep template-specific source values in `rows[].derivations`.

Pilot 3 ADADAS has two row templates, `collected` (one row per selected
QS record) and `planned` (one row per missing ACTOT visit slot). The
templates supply only source-specific values; everything shared is
derived once at column level:

```yaml
columns:
  - name: TRTSDT
    type: date
    label: Date of First Exposure to Treatment
    derivation: {source: ADSL_LOOKUP.TRTSDT}

  - name: ADY_RAW
    type: float
    label: Ady Raw (helper)
    derivation:
      study_day: {date: ADT_RAW, reference: TRTSDT}

  - name: AVISIT
    type: str
    label: Analysis Visit
    derivation:
      case:
        - when: "ROWSRC = 1"
          then: {source: PLAN_AVISIT}
        - otherwise:
            cut:
              source: ADY_RAW
              breaks: [1, 84, 140]
              labels: ["Baseline", "Week 8", "Week 16", "Week 24"]
              right: true

rows:
  - id: collected
    dataset: QS
    filter: "QS.QSTESTCD IN ('ACITM01', 'ACTOT')"
    derivations:
      ROWSRC: {literal: 0}
      PLAN_AVISIT: {literal: null}
      ADT_RAW:
        to_date: {source: QS.QSDTC}

  - id: planned
    dataset: PLAN
    derivations:
      ROWSRC: {literal: 1}
      PLAN_AVISIT: PLAN.AVISIT
      ADT_RAW: {literal: null}
```

Two constraints govern placement. First, phases: row construction runs
before column derivation, so a row derivation cannot read a value
produced only during the column phase, while a column derivation
normally reads row-built values. A row-local column derivation (no
lookup, aggregate, window, intermediate, or non-row-local column read)
becomes the entry default when a `rows` entry names that column. Second,
declaration order is load-bearing within `columns`: every dependency
must refer to a column declared earlier, evaluated in declaration order.
Break long chains into named helper columns (e.g. ADSL `EXENDT_RAW` then
`EXENDT_CAPPED` then `EXDURD`) rather than one long expression.

## 5. Derivation order and dependency handling

Derive in dependency order; every predecessor is staged before it is read:

1. **ADSL** - from SDTM (per-record exposure math lives in the
   `TRTEDT`/`CUMDOSE` aggregate `derive` bindings over `EX`, not in a
   staging spec).
2. **Other ADaM** - each from SDTM, the derived
   `adsl-yamaa.parquet`, and any derived predecessors it needs (e.g. ADTTE reads
   the derived ADAE; ADLBC reads the derived ADSL).

"One spec per output" means no `adsl-s1.yaml` fragments: ordered window
chains (`rank` -> `case` -> `previous_non_missing`), per-record exposure
helpers, and occurrence sequence numbers stay as derivations inside the
single dataset spec. Upstream row creation the spec cannot do (e.g. the
222 LOCF slots in ADADAS) happens in `python/make_plan.py`, whose pinned
CSV the spec reads as ordinary input -- that is orchestration, not a
staging spec.

Compare every output column, label, and cell against the reference with
the declared tolerance. `compare.py` checks row alignment on the dataset
keys, numeric cells at `|derived - official| <= 1e-10`, character cells
exactly (with null and `""` normalized), and every YAML `label`
against the official parquet field metadata. It prints matched/total
columns and cells per dataset plus the total, and exits nonzero on any
mismatch.

## 6. When one specification is blocked

Do not work around a missing language or runtime capability by committing
multiple YAML files for one output dataset. Instead:

1. Reduce the blocker to the smallest single-spec example and verify it against
   the latest yamaa revision.
2. If it still fails, create a yamaa GitHub issue with the `pilot-dry-run`
   label. Include the reduced specification, the diagnostic or timeout, and
   the required before/after behavior.
3. Leave the output dataset out of the study run until that issue is resolved;
   do not commit its staged workaround as the intended derivation.
