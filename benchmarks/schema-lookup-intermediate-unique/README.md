# End-of-Study Record

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/schema-lookup-intermediate-unique.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** derive each subject's end-of-study date from their
disposition records, checking that no subject has more than one
end-of-study record.

**Input:** one `spec.yaml` declaring two datasets. `DM` carries one
row per subject. `DS` carries disposition records: subject, category,
decoded term, and start date.

**Lookups:**

- `DS_EOS` names each subject's end-of-study record: a disposition
  event that is not a screen failure. The subject key is shared with
  the output, so it is inferred rather than restated. `EOSDT` is that
  record's start date; a subject with no such record, such as one who
  only failed screening, keeps `EOSDT` blank.

**Note:** the eligible records are checked for a repeated subject
before any row is built, so the lookup needs no rule for choosing
among them: a subject with two end-of-study records stops the run
rather than taking either one. A screen-failure record is never
eligible, so it may sit beside a subject's end-of-study record.

**Standard:** ADaM | **Domain:** ADSL
