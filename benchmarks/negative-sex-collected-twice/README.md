# Reject a subject whose sex was collected twice

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-sex-collected-twice.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** build one Demographics (DM) record per subject carrying sex
(SEX) and age (AGE).

**Input:** EDC output in long form, one row per collected item. The
screening visit was entered twice for one subject, and the sex question
was answered differently each time.

**Variables:**

- **SEX** would be the recorded sex coded `M` (Male) or `F` (Female), but
  a subject with two different answers has no single recorded sex and one
  record cannot carry both, so the run is rejected and no artifact is
  accepted.
- **AGE**: age in whole years as collected; blank when missing.

**Standard:** SDTM | **Domain:** DM

## How to fix

Decide which answer stands for the subject. Two different sexes for one
subject is usually a data issue to query and correct at the source, and
correcting it there leaves one answer and needs nothing here.

When both entries are legitimate and the later one corrects the earlier,
say so: order the answers as they were collected and keep the last, in a
working column the coded value then reads.

```yaml
- name: SEXCOLL
  type: str
  derivation:
    source:
      filter: ODM.ItemOID = 'IT.DM.SEX'
      variable: ODM.Value
      multiple_matches:
        order_by: [ODM.StudyEventRepeatKey]
        keep: last

- name: SEX
  type: str
  derivation:
    mapping:
      source: SEXCOLL
      dict:
        Male: M
        Female: F
```

When one entry is the record of the visit and the other is a duplicate to
be ignored, name the entry the record reports instead, for example with
`ODM.ItemOID = 'IT.DM.SEX' AND ODM.StudyEventRepeatKey = 1`.

Do not widen the keys to keep both answers: a subject whose visit was
entered twice is still one demographics record.
