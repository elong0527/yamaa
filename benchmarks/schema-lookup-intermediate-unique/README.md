# End-of-Study Record

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/schema-lookup-intermediate-unique.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** derive each subject's end-of-study date from their
disposition records, asserting that each subject carries exactly one
end-of-study record.

**Input:** one `spec.yaml` declaring two datasets. `DM` carries one
row per subject. `DS` carries disposition records: subject, category,
decoded term, and start date.

**Lookups:**

- `DS_EOS` names each subject's end-of-study record. Only disposition
  events that are not screen failures are eligible; the subject key is
  shared with the output, so it is inferred rather than restated. The
  spec asserts the subject key is unique across the eligible records,
  so no ordering or keep is needed to choose among them: a repeated
  key fails the run instead of resolving ambiguously. A subject with
  no eligible record keeps `EOSDT` blank.

**Standard:** ADaM | **Domain:** ADSL
