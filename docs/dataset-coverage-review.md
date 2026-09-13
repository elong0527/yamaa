---
title: Dataset coverage review
---

# Dataset coverage review

> **YAMAA docs:** [Why](why-yamaa.md) | [Excel to YAMAA](excel-to-yamaa.md) | [Schema concepts](schema-concepts.md) | [Examples walkthrough](yaml-examples-walkthrough.md) | [Dataset coverage review](dataset-coverage-review.md)

> **Read this if** you are reviewing whether the examples for one dataset --
> ADSL, ADAE, LB -- cover what a real specification for that dataset has to
> handle, and whether each example's README, specification and expected output
> agree with one another. Where the
> [examples walkthrough](yaml-examples-walkthrough.md) is organized by question,
> this page reads all of one dataset's examples together. Sections 1 to 4 are
> the method; section 5 records the reviews, one question at a time.

---

## 1. Which examples belong to a dataset

Every example in `yaml/examples/`, positive or negative, goes under the dataset
its folder name carries: `adam-adsl-age-group` and
`negative-adsl-cyclic-parent` both go under ADSL. Special cases:

- **The folder name carries no dataset**: use `domain:` in its `spec.yaml`.
  [`odm-form-scoped-item-resolution`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/odm-form-scoped-item-resolution)
  declares LB, and goes under LB.
- **Such a negative only borrows the domain** for a language-wide failure: it
  is still listed, but marked *hosted*, and C1 does not count it as coverage of
  that dataset.
  [`negative-dataset-path-url`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/negative-dataset-path-url)
  declares ADLB, but a source given as a web address says nothing about ADLB.
- **The declared domain is not a dataset**:
  [`negative-function-contract-mismatch`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/negative-function-contract-mismatch)
  declares `TEST`, and is listed under *No dataset*.

---

## 2. The four criteria

Each criterion is a question and a short method. Everything a criterion raises
goes into the record's one findings table; the criterion itself carries only
what a reader needs beside it. C1 to C4 are not the rules R001 to R023.

### C1. Covered situations, and what is missing

**Question.** Which situations do this dataset's examples cover, and which
scope it needs is missing?

**Method.**

1. One row per example: its situation in one short sentence, in data terms.
2. Mark whether the situation is a **real case** of this dataset's mapping. An
   example whose situation a real mapping never has -- a language feature
   dressed in the dataset -- is marked `No`, is not counted as coverage, is a
   **rework example** finding, and skips C2 to C4.
3. A missing scope that matters for this dataset is an **add example** finding:
   what is missing and why, in one sentence, and the proposed example name.
4. A case common to every dataset -- a missing value, a duplicate record -- is
   one line, not a finding.

### C2. README against its folder

**Question.** Does the README say what the example's specification, input and
expected output actually do?

**Method.** A quick check, not an audit. Read the README beside the folder and
raise a finding only where they disagree: a statement the output contradicts,
behavior the output shows that the README leaves out, a statement only the
sample makes true (name the input that breaks it), or a breach of the data
contract in
[`yaml/examples/agents.md`](https://github.com/elong0527/yamaa/blob/main/yaml/examples/agents.md).
What agrees is not listed.

### C3. The specification behind the situation, beside its Excel form

**Question.** Which schema constructs carry the example's situation, and how
does the specification read as an Excel spec?

**Method.**

1. Copy into the record **only the excerpt of the specification behind the
   example's situation** in C1 -- not the whole file -- and annotate it: a
   comment names the construct's rule, the Excel cell a line fills, or
   `no Excel cell`. Lines outside the situation are left out, and the excerpt
   says so. For an inherited example, excerpt `expected/resolved.yaml` and
   name each column's layer.
2. Write the whole specification's Excel form into `excel-spec.md` in the
   example folder, as Markdown tables filled only from the specification: a
   Dataset sheet row, the eleven-column Variable sheet of
   [Excel to YAMAA](excel-to-yamaa.md), section 1, and a VLM sheet when there
   are row templates.
3. A conformance problem the Excel form exposes is a finding.

### C4. Expected output traced to input

**Question.** Does the expected output follow from the input through the
specification?

**Method.** Recompute the expected values from the input under the rules --
never from the expected file -- and compare. For a negative example, show that
the input and specification path produce the `phase` and `condition` in
`expected/error.yaml`. Record one line on how it was checked. A value that does
not reproduce, or an unexercised path that matters, is a finding.

---

## 3. Verdicts, findings and the record template

### 3.1 Verdicts

| Verdict | Meaning |
|---|---|
| **Pass** | No finding |
| **Partial** | Findings, but no example is wrong -- a missing scope, a behavior the README leaves out, a statement only the sample makes true |
| **Fail** | An example is wrong -- a README the output contradicts, a value that does not reproduce |
| **Not reviewed** | Not asked yet |

### 3.2 Findings

Each finding has an ID `<DATASET>-C<n>-<nn>` (`ADSL-C2-01`) and one action: fix
README, fix specification, fix expected output, add example, rework example
(recast it as a real case, or rename it off the dataset), or design finding
when the language cannot express it. The fix is a separate change; the finding
names where it went.

### 3.3 Record template

Copy under section 5.2 the first time a dataset is asked about.

~~~markdown
### <DATASET>

**Examples:** `...`; hosted: `...`
**Reviewed at:** YYYY-MM-DD, commit `abcdef0`

#### C1 -- <verdict>

| Example | Situation | Real case |
|---|---|---|

Common, not raised: ...

#### C2 -- <verdict>

<One line: which findings, or that the READMEs agree with their folders.>

#### C3 -- <verdict>

<For each example: the annotated excerpt behind its situation, and its excel-spec.md.>

#### C4 -- <verdict>

<One line: how it was checked, and whether it reproduced.>

#### Findings

| ID | Example | File:line | Finding | Action |
|---|---|---|---|---|
~~~

---

## 4. How a review is recorded

1. A question names a dataset, and optionally an example and a criterion:
   "ADSL, C1".
2. The answer comes from the current commit and cites its files. It is recorded
   under section 5.2, and the dataset's row in section 5.1 is updated.
3. A re-check updates the record in place with the new date and commit; a
   changed verdict keeps one line on why.
4. The page is ASCII English, as the repository's lint requires.

---

## 5. Reviews

### 5.1 Dataset index

| Dataset | Standard | Examples (positive + negative) | C1 | C2 | C3 | C4 | Reviewed at |
|---|---|---|---|---|---|---|---|
| [AE](#ae) | SDTM | 2 + 0 | Partial | Pass | Partial | Pass | 2026-09-11, `55d4a9c` |
| DM | SDTM | 3 + 1 | Not reviewed | Not reviewed | Not reviewed | Not reviewed | -- |
| DS | SDTM | 1 + 0 | Not reviewed | Not reviewed | Not reviewed | Not reviewed | -- |
| EX | SDTM | 1 + 0 | Not reviewed | Not reviewed | Not reviewed | Not reviewed | -- |
| FA | SDTM | 1 + 0 | Not reviewed | Not reviewed | Not reviewed | Not reviewed | -- |
| LB | SDTM | 6 + 3 | Not reviewed | Not reviewed | Not reviewed | Not reviewed | -- |
| RELREC | SDTM | 1 + 0 | Not reviewed | Not reviewed | Not reviewed | Not reviewed | -- |
| SUPPMH | SDTM | 2 + 0 | Not reviewed | Not reviewed | Not reviewed | Not reviewed | -- |
| VS | SDTM | 2 + 2 | Not reviewed | Not reviewed | Not reviewed | Not reviewed | -- |
| ADAE | ADaM | 15 + 14 | Not reviewed | Not reviewed | Not reviewed | Not reviewed | -- |
| ADCE | ADaM | 1 + 0 | Not reviewed | Not reviewed | Not reviewed | Not reviewed | -- |
| ADCM | ADaM | 1 + 0 | Not reviewed | Not reviewed | Not reviewed | Not reviewed | -- |
| ADEG | ADaM | 3 + 1 | Not reviewed | Not reviewed | Not reviewed | Not reviewed | -- |
| ADEX | ADaM | 3 + 5 | Not reviewed | Not reviewed | Not reviewed | Not reviewed | -- |
| ADLB | ADaM | 8 + 23 | Not reviewed | Not reviewed | Not reviewed | Not reviewed | -- |
| ADOE | ADaM | 1 + 0 | Not reviewed | Not reviewed | Not reviewed | Not reviewed | -- |
| ADQS | ADaM | 1 + 0 | Not reviewed | Not reviewed | Not reviewed | Not reviewed | -- |
| ADRS | ADaM | 6 + 2 | Not reviewed | Not reviewed | Not reviewed | Not reviewed | -- |
| ADSL | ADaM | 21 + 28 | Not reviewed | Not reviewed | Not reviewed | Not reviewed | -- |
| ADTR | ADaM | 2 + 0 | Not reviewed | Not reviewed | Not reviewed | Not reviewed | -- |
| ADTTE | ADaM | 4 + 0 | Not reviewed | Not reviewed | Not reviewed | Not reviewed | -- |
| ADVS | ADaM | 8 + 5 | Not reviewed | Not reviewed | Not reviewed | Not reviewed | -- |
| No dataset | -- | 0 + 1 | Not reviewed | Not reviewed | Not reviewed | Not reviewed | -- |

### 5.2 Records

### AE

**Examples:** `sdtm-ae-dictionary-coding`, `sdtm-ae-effective-transaction`; hosted: none
**Reviewed at:** 2026-09-11, commit `55d4a9c`

#### C1 -- Partial

| Example | Situation | Real case |
|---|---|---|
| `sdtm-ae-dictionary-coding` | A reported term is coded by exact match to a dictionary lowest-level term; an unmatched or blank term reads `NOT CODED` | Yes |
| `sdtm-ae-effective-transaction` | An event takes its values from its latest transaction, by timestamp then sequence; a removed event is not delivered | No -- AE-C1-04 |

Common, not raised: sequence numbering, controlled-term recoding, supplemental
qualifiers and study day, each shown by another SDTM example.

#### C2 -- Pass

`sdtm-ae-dictionary-coding` agrees with its folder.

#### C3 -- Partial

`sdtm-ae-dictionary-coding/spec.yaml`, excerpt, annotated:

```yaml
# Excerpt: the lines that code the reported term. spec.yaml has the rest.
datasets:                           # no Excel cell
  AE_RAW: input/ae_raw.csv          # untyped: every field str (R014)
  MEDDRA: input/meddra_26_1.csv     # the dictionary is an ordinary input
base: AE_RAW                        # Structure: one record per AE_RAW record (R001)
metadata:                           # annotation only, never validated
  dictionary: MedDRA
  dictionary_version: "26.1"        # nothing ties the input file to it

columns:                            # Variable sheet
  - name: AEDECOD
    type: str
    label: Dictionary-Derived Term
    derivation:
      mapping_from:                 # exact, case-sensitive match (R007, R019)
        source: AE_RAW.AETERM
        dataset: MEDDRA
        key: LLTNAME                # must be unique, else duplicate_lookup_key
        value: PTNAME
        missing: NOT CODED          # blank term (R008): no Excel cell
        unmapped: NOT CODED         # term not found (R008): AE-C3-01
  # AEBODSYS: the same mapping_from with value: SOCNAME. The match is stated
  # twice; a record lookup (R015) states it once but cannot answer NOT CODED.
```

Excel form: `sdtm-ae-dictionary-coding/excel-spec.md`.

#### C4 -- Pass

`sdtm-ae-dictionary-coding`: every value recomputed from the input by a
standalone script applying the rules; the artifact reproduces row for row.

#### Findings

| ID | Example | File:line | Finding | Action |
|---|---|---|---|---|
| AE-C1-01 | -- | -- | No MedDRA hierarchy from the coder's chosen term code (`AELLT`, `AEPTCD`, `AEHLT`, `AEHLGT`, `AESOC` and codes), with the primary body system chosen among a preferred term's several; an exact text match codes only terms already spelled as in the dictionary | Add example `sdtm-ae-meddra-hierarchy` |
| AE-C1-02 | -- | -- | No partial `AESTDTC` or `AEENDTC` kept at its collected precision, with study day empty; every SDTM example's dates are complete | Add example `sdtm-ae-partial-dates` |
| AE-C1-03 | -- | -- | No `AESER` checked against its criteria (`AESDTH`, `AESHOSP`, ...); no example derives or checks it | Add example `sdtm-ae-seriousness-criteria` |
| AE-C1-04 | `sdtm-ae-effective-transaction` | -- | Not a real SDTM AE case: the EDC extract an AE mapping reads already holds each record's current state, so no SDTM specification replays a transaction log | Rework example: recast as a real AE case, or rename it off AE |
| AE-C3-01 | `sdtm-ae-dictionary-coding` | spec.yaml:56-57, 68-69 | `NOT CODED` is not a MedDRA 26.1 term, yet it fills `AEDECOD` and `AEBODSYS`; a dictionary conformance check flags every such row | Fix README: `NOT CODED` marks an event coding must resolve before delivery |
