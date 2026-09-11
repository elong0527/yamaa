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

Each criterion is a question and a method; section 3.3 fixes what is recorded.
C1 to C4 are not the rules R001 to R023.

### C1. Covered situations, and what is missing

**Question.** Which situations do this dataset's examples cover, and is a
scope this dataset needs missing?

**Method.**

1. List the covered situations, one row per example, stated in data terms --
   "a subject with no exposure falls back to the planned arm".
2. Judge the gaps for this dataset only, when it is reviewed. Raise the scopes
   that matter for it -- not every small case.
3. A common case that is not covered -- a missing value, a duplicate record --
   gets one line beside the list saying it is common to every dataset, and so
   is neither raised per dataset nor given its own example.
4. A scope this dataset is missing goes into **To be evaluated and added**: what
   is missing, why it matters for this dataset, and a proposed example name.

### C2. README consistency, for each covered situation

**Question.** Does the README agree with what the example shows, and does the
specification guarantee what the README says?

**Method.** For each situation:

1. **Against the expected output**, mark it **Consistent**, **Inconsistent**, or
   **Silent**:
   - every column that carries logic has a bullet, in artifact order;
   - each bullet, including what it says for unsupported input, matches the
     expected values;
   - the title names the input files and the output grain;
   - the expected output shows no behavior the README leaves out;
   - the README keeps the data contract in
     [`yaml/examples/agents.md`](https://github.com/elong0527/yamaa/blob/main/yaml/examples/agents.md).
2. **Against the specification**, name the lines that enforce each README
   statement and mark it **Guaranteed** (by construction or a declared
   verification), **Sample-only** (name an input outside the sample that breaks
   it), or **Not met**.
3. Check `keys` against the stated grain, and that verifications assert what
   the derivation guarantees rather than what the sample shows.

### C3. How the specification is built from the schema, beside its Excel form

**Question.** Which schema constructs build the specification, and how does it
read as an Excel spec?

**Method.**

1. For each top-level field used, and each column in declaration order, name
   the construct -- type, verb, handler, check -- with its schema file and rule.
   For an inherited example, read `expected/resolved.yaml` and note which layer
   gives each column.
2. Write the Excel form in the template of [Excel to YAMAA](excel-to-yamaa.md),
   section 1: a Dataset sheet row, the eleven-column Variable sheet, and a VLM
   sheet when there are row templates. Fill cells only from the specification.
3. List what has no Excel cell: input files, handlers, verifications,
   overrides, dependency order versus delivery order.

### C4. How the expected results connect to the input through the specification

**Question.** Does every expected row and value trace back to the input through
the specification?

**Method.**

1. **Rows.** Name the input records behind each expected row, and why the row
   count and order are what they are.
2. **Values.** For one row per situation, and every row that reaches a branch,
   handler or boundary, trace each non-key value: input value, derivation step,
   expected value. Recompute it from the input; the expected file is not its
   own evidence.
3. **Negative examples.** Show that the input and specification path named in
   `expected/error.yaml` produce its `phase` and `condition`.
4. **Unexercised logic.** List branches or handlers no row reaches; raise one
   in C1 only if it matters for this dataset.

---

## 3. Verdicts, findings and the record template

### 3.1 Verdicts

| Verdict | Meaning |
|---|---|
| **Pass** | No finding |
| **Partial** | Findings, but no example is wrong -- a missing scope, a silent README, a Sample-only guarantee |
| **Fail** | An example is wrong -- an inconsistent README, an unmet requirement, a value that does not reproduce |
| **Not reviewed** | Not asked yet |

### 3.2 Findings

Each finding has an ID `<DATASET>-C<n>-<nn>` (`ADSL-C2-01`) and one action: fix
README, fix specification, fix expected output, add example, or design finding
when the language cannot express it. The fix is a separate change; the finding
names where it went.

### 3.3 Record template

Copy under section 5.2 the first time a dataset is asked about.

```markdown
### <DATASET>

**Examples:** `...`; hosted: `...`
**Reviewed at:** YYYY-MM-DD, commit `abcdef0`

#### C1 -- <verdict>

| Example | Situation |
|---|---|

Common, not raised: ...

| To be evaluated and added | Why it matters here | Proposed example |
|---|---|---|

#### C2 -- <verdict>

| Example | README statement | Against output | Against specification | Evidence |
|---|---|---|---|---|

#### C3 -- <verdict>

| Field or column | Construct | Schema file | Rule |
|---|---|---|---|

| Dataset | Description | Class | Structure | Key Variables |
|---|---|---|---|---|

| Variable Name | Variable Label | Type | Length | Controlled Terms or Format | Origin | Core | Conversion Definition | Variable Type | Variable Order | Comments for Define |
|---|---|---|---|---|---|---|---|---|---|---|

| VLM: Variable | Where Clause | Conversion Definition |
|---|---|---|

No Excel cell: ...

#### C4 -- <verdict>

| Expected row | Driven by |
|---|---|

| Column | Input | Step | Expected | Reproduced |
|---|---|---|---|---|

Unexercised: ...

#### Findings

| ID | Example | File:line | Finding | Action |
|---|---|---|---|---|
```

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

| Dataset | Standard | C1 | C2 | C3 | C4 | Reviewed at |
|---|---|---|---|---|---|---|

### 5.2 Records

No dataset has been reviewed yet.
