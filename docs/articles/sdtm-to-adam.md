# SDTM to ADaM derivation recipe

Derive ADaM datasets from SDTM with one specification per output dataset. The
[introduction](intro.md) explains the language; the
[style guide](style.md) owns authoring conventions.

- create exactly one spec per output dataset (e.g. `adsl.yaml`); keep temporary
  values and ordered calculation steps as derivations in that spec, not as
  `adsl-s1.yaml`, `adsl-final.yaml`, or other staged dataset files
- establish dependency to avoid duplicate logic: the rest of the ADaM specs
  link `adsl.yaml` through `schema` and copy its variables
  ([section 3](#3-derivation-order-and-dependency-handling))
- keep orchestration small: list every spec in the `adam` section of the
  study's `project.yaml` and build them with one project run,
  `project.run("adam")` or `yamaa run project.yaml --section adam`, not one
  call per dataset; see the
  [R, Python and command-line API](api.md#project-execution)
- keep study-owned orchestration and comparison code beside the study, and
  the comparison separate from the orchestration
- when the specs replace existing derivation programs, build with the yamaa
  version the study pins and compare each dataset with the one those programs
  produce, within the numeric tolerance the study states: report matched and
  total columns and cells per dataset, and count any mismatch or validation
  issue as a failure

Existing programs show what each dataset means; write the spec from that
intent rather than translating each program step by step.

## 1. Prerequisites

- **Input** The project's `data/sdtm/` holds the staged SDTM
  inputs as parquet, one file per domain (e.g. `dm.parquet`, `ae.parquet`).
- **yamaa** Install the supported [Python](https://github.com/elong0527/yamaa/blob/main/python/README.md)
  or [R](https://github.com/elong0527/yamaa/blob/main/R/yamaa/README.md) package.

## 2. Spec authoring conventions

- **Every spec declares its model.** Each spec starts with `yamaa: "1.0"` and
  `type: spec` and declares `model: ADaM`. The project's `adam` section lists
  only ADaM specs, and names the implementation guide in `standards`.
- **Uniform derivations belong at column level.** A derivation identical
  across every row template (group-key echoes like `STUDYID`/`USUBJID`,
  literals like `DOMAIN`, `PARAMCD` assignments) goes in
  `columns[].derivation`, never inside `rows:` derivations. Row templates
  should be minimized for section level details. An `aggregate`, a window
  derivation, a read through a named intermediate, or a driver read when
  templates build from different datasets depends on its template's row
  construction and stays in its row template.
- **Read the input dataset directly.** A named intermediate whose `key`
  restates its input's `key`, with no `filter`, selection or handler that
  changes behavior, reads what the implicit join already reads; qualify the
  input dataset where it is read instead of aliasing it.
- **Declaration order is load-bearing.** A column must be declared after any
  column its derivation references.
- **Ordered window chains stay in the dataset spec.** A later window derivation
  may read an earlier window result, including through intervening scalar
  derivations. For example, `rank` -> `case` -> `locf` is one
  ordered derivation chain in one row template; it does not require staging
  files.
- **ADSL is derived once and consumed downstream.** Subject-level variables
  are derived only in the ADSL spec. The other specs read them from the
  derived `adsl-yamaa.parquet` predecessor: the input links it to `adsl.yaml`
  through `schema` and declares its join `key`, and a `copy` entry takes the
  variables, each with the type, label and Predecessor origin `adsl.yaml`
  declares.
- **Every column declares `label:`**, unless a `copy` entry takes it from the
  producer.
- **Write each spec in the specification style.** Field order, blank lines,
  line width, the canonical spellings and the authoring findings (uniform
  row-template derivations, redundant intermediates, missing output-column
  labels, unused input keys, equality filters) are fixed by the
  [style guide](style.md).
  Run `yamaa.lint("<spec>.yaml", fix=True)` (`yamaa_lint("<spec>.yaml", fix = TRUE)`
  in R, `yamaa lint --fix <spec>.yaml` on the command line) with the pinned
  yamaa, then correct the rows in the result's `findings` table.
- **Output naming.** Every spec's `output.path` is `adam/<ds>-yamaa.parquet`
  (e.g. `adam/adsl-yamaa.parquet`).

## 3. Derivation order and dependency handling

Derive in dependency order; every predecessor is staged before it is read. A
project run keeps this order itself when each input that reads a predecessor
links that predecessor's spec through `schema`:

1. **Staging specs** (e.g. per-record exposure staging) - pure SDTM input.
2. **ADSL** - from SDTM plus staging specs.
3. **Other ADaM** - each from SDTM, the derived
   `adsl-yamaa.parquet`, and any derived predecessors it needs (e.g. ADTTE reads
   the derived ADAE; ADLBC reads the derived ADSL).

This ADAE spec reads ADSL as a predecessor. Its `ADSL` input names the file
the ADSL spec writes and, through `schema`, the spec itself; `key` states how
an ADSL record joins an adverse event; and one `copy` entry takes four
subject-level variables:

```yaml
yamaa: "1.0"
type: spec
model: ADaM
domain: ADAE
keys: [STUDYID, USUBJID, AESEQ]
input:
  AE: data/sdtm/ae.parquet
  ADSL:
    path: adam/adsl-yamaa.parquet
    schema: adsl.yaml
    key: [STUDYID, USUBJID]
base: AE

output:
  path: adam/adae-yamaa.parquet
  columns: [STUDYID, USUBJID, AESEQ, AETERM, AGE, SEX, TRT01A, TRTSDT]

columns:
  - name: STUDYID
    type: str
    label: Study Identifier
    derivation: AE.STUDYID

  - name: USUBJID
    type: str
    label: Unique Subject Identifier
    derivation: AE.USUBJID

  - name: AESEQ
    type: int
    label: Sequence Number
    derivation: AE.AESEQ

  - name: AETERM
    type: str
    label: Reported Term for the Adverse Event
    derivation: AE.AETERM

  # Each copied column takes its type and label from adsl.yaml, and its
  # Predecessor origin names ADSL.<column>.
  - copy:
      dataset: ADSL
      columns: [AGE, SEX, TRT01A, TRTSDT]
```

A copied column is an ordinary column of ADAE: a later column may read it,
such as a study day that reads `TRTSDT`, and it is published only when
`output.columns` lists it. The link reads only what `adsl.yaml` declares about
its output; it never runs ADSL. The project lists both specs, and its run
builds ADSL first because ADAE links it:

```yaml
yamaa: "1.0"
type: project
adam:
  specs: [adsl.yaml, adae.yaml]
  standards:
    - {name: ADaMIG, version: "1.3", status: Final}
```

## 4. When one specification is blocked

Do not work around a missing language or runtime capability by committing
multiple YAML files for one output dataset. Instead:

1. Reduce the blocker to the smallest single-spec example and check it with
   the latest yamaa release.
2. If it still fails, open a yamaa GitHub issue with the bug report or change
   request form, a public or invented reproduction, the diagnostic or
   timeout, and the required before/after behavior. Follow the
   [private-study feedback process](https://github.com/elong0527/yamaa/blob/main/CONTRIBUTING.md#reporting-findings-from-private-studies)
   before publishing study-derived findings.
3. Leave the output dataset out of the study run until that issue is resolved;
   do not commit its staged workaround as the intended derivation.
