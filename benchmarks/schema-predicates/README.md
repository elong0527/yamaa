# Predicates

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/schema-predicates.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** pin three-valued predicate logic and `case` evaluation over
nullable inputs: which branch a record takes when a comparison is
unknown rather than false.

**Input:** `DM` carries six subjects with nullable ages and treatment
group codes, including a missing age, a missing treatment group, and
lowercase codes that must not match uppercase patterns.

**Columns:**

- `AGERISK` takes the first `case` branch whose predicate is true. The
  70-year-old matches two age branches and takes the first. A missing
  age is unknown in both comparisons, never false, so it falls through
  to the fallback. The 40-year-old lands in the middle band, pinning the
  inclusive endpoint.
- `SENIOR_M` combines `NOT` with `AND`. `NOT` of an unknown comparison
  is unknown, so the subject with the missing age skips the first branch
  and is caught by the explicit missingness test, while the subjects
  whose predicate is plainly false fall through to the fallback.
- `TRTCLASS` pins case-sensitive `LIKE` (`drug-low` matches nothing,
  `DRUGX` matches `DRUG%`), `NOT IN` over a missing value (unknown, not
  true, so the subject with no treatment group skips to the missingness
  branch), and the bare-source shorthand, which copies the matched
  screening code through unchanged.

**Standard:** CDISC | **Domain:** ADSL
