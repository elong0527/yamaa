# Fatal Event Join

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/schema-lookup.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** derive each subject's death study day and cause of death by
joining their adverse events, and code the cause against a medical
dictionary.

**Input:** one `spec.yaml` declaring three datasets. `DM` carries one
row per subject. `AE` carries the adverse-event records: subject,
reported term, outcome, and study day. `MEDDRA` carries the dictionary:
each reported term beside its preferred term.

**Lookups:**

- `DEATHEV` names each subject's fatal adverse event. Only fatal
  records are eligible, and the latest study day wins; the subject key
  is shared with the output, so it is inferred rather than restated. A
  subject with two fatal records takes the later one's day and term,
  and a subject with no fatal record keeps `DTHDY` and `DTHCAUS`
  blank.
- The inline lookup codes the reported cause into `DTHPTERM`: the
  subject's cause is matched against the dictionary's reported term and
  the preferred term comes back. A missing cause matches nothing, so
  the coded cause stays blank too.

**Note:** a join never changes the row count: every subject keeps
exactly one row, and every read of the same lookup sees the one
selected record. When no record is selected, each read answers with a
blank rather than failing.

**Standard:** ADaM | **Domain:** ADSL
