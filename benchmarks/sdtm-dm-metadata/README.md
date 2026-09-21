# Metadata Contract

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/sdtm-dm-metadata.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** build one DM record per subject carrying `SITEID`, `AGE`,
`AGEU`, `SEX`, and `COUNTRY`.

**Input:** one row per subject carrying `SITEID`, `AGE`, `SEX`, and
`COUNTRY`.

**Variables:**

- `SITEID` is the collected site identifier.
- `AGE` is the collected age in whole years, between 0 and 120;
  missing when not collected.
- `AGEU` is fixed to `YEARS`, since age is collected in whole
  years.
- `SEX` is the collected sex, one of `F`, `M`, or `U`.
- `COUNTRY` is the collected three-letter country code.

**Note:** the dataset and its variables carry the description,
labels, provenance, lengths, terminology, class, structure, and
standard version a review needs, and the benchmark carries the
data-definition document those declarations produce beside the data
itself; a combined identifier longer than 30 characters is rejected
rather than shortened. The same declarations also produce the
exchange file `dm.json`, which carries the rows and the column
metadata together and is the file the document points at.

**Standard:** SDTM | **Domain:** DM
