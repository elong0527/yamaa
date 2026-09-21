# Reject Transport Container Output

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-output-transport.html) [![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** attempt one record for every subject carrying the collected
age (`AGE`) and sex (`SEX`), written to a SAS Transport file.

**Input:** collected demographics records carrying age (`AGE`) and
sex (`SEX`).

**Variables:**

- `AGE` would be the collected age, carried into the result
  unchanged.
- `SEX` would be the sex collected in the demographics records,
  carried into the result unchanged.

**Note:** the result file named here is a SAS Transport file. That
container bounds names to eight uppercase characters, labels to
forty, and text to two hundred bytes, and it has no whole-number
type and no calendar type, so it cannot hold everything this
language admits. A transport file is produced by the packaging step
that assembles a submission from published results, not by this
run, so the run is rejected before any data is read and no result
is accepted.

**Standard:** ADaM | **Domain:** ADSL

## How to fix

Name a file this language writes: `.csv` for a reviewable artifact, or
`.parquet` for a production one that carries its own types.

```yaml
output:
  path: adsl.parquet
  columns: [STUDYID, USUBJID, AGE, SEX]
```

Convert the published artifact to SAS Transport in the packaging step that
assembles the submission. The name, label, text, number, and date limits the
container imposes are decided there, against the submission being built, and
a name or a value the container cannot carry is a packaging failure rather
than a silent change to the dataset.
