# Predicates

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/schema-predicates.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** show which branch of a first-match `case` a record takes when
an input is missing: a comparison with a missing value is unknown
rather than false, and only a true condition selects a branch.

**Input:** one `spec.yaml` over `DM`, carrying each subject's age, sex,
and treatment group code. Age and group may be missing, and a code may
be written in lower case.

**Columns:**

- `AGERISK` is `HIGH` over 65, `MEDIUM` from 40 through 65, and `LOW`
  otherwise. The first true branch wins, so an age over 65 is `HIGH`
  although it also passes the test for 40. A missing age is unknown in
  both tests, never true, so it falls through to `LOW`.
- `SENIOR_M` is `Y` for a male younger than 65 (its age test is
  negated), `U` when age is missing, and `N` otherwise. Negating an
  unknown comparison leaves it unknown, so a missing age never reaches
  `Y` and is caught by the explicit missing-age test instead.
- `TRTCLASS` copies a screening code (one starting `SCRN`) through
  unchanged, and is `DRUG_FAMILY` for a code starting `DRUG`,
  `STANDARD` for `PLACEBO`, `MISSING` when the code is missing, and
  `OTHER` for any other code. Pattern matching is case-sensitive, so a
  lower-case `drug-low` is `OTHER`. A missing code is unknown, not
  true, in the test for "neither `DRUG` nor `PLACEBO`", so it reaches
  `MISSING`.

**Note:** a branch is taken only when its condition is true. A
comparison with a missing value is unknown, and so is its negation, so
a missing value never satisfies a "not" test either; only an explicit
test for missingness catches it.

**Standard:** CDISC | **Domain:** ADSL
