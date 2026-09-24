# Reject Conflicting Ages

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-keys-conflict.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** build one subject-level record carrying the collected age
(`AGE`) and sex (`SEX`), one for each subject the collected
demographics carry.

**Input:** collected demographics records carrying age (`AGE`) and
sex (`SEX`), where a subject may be collected on more than one record.

**Variables:**

- `AGE` would be the collected age, carried into the result
  unchanged.
- `SEX` would be the collected sex, carried into the result
  unchanged.

**Note:** the subject decides how many records come out, so a subject
collected twice is still one record and both collected records feed
it. A value both records report the same way, or that only one of
them reports, is one answer and carries without complaint. Two
different ages are two answers to a question the record has one
place for, so the run fails where the age is read and no artifact is
accepted.

**Standard:** ADaM | **Domain:** ADSL

## How to fix

Reconcile the two collected records and correct the governed demographics
input so it holds one supported age for the subject. If both records are
legitimate, declare which one answers, for example the one collected last,
rather than leaving the choice to file order:

```yaml
- name: AGE
  type: int
  derivation:
    source:
      variable: DM.AGE
      multiple_matches:
        order_by: [DM.DMDTC]
        keep: last
```

The sample records carry no collection date (`DMDTC` here), so the source has
to supply one first.

Do not reach for a row template to make the two records two rows. `keys`
states one record per subject, so a second row for that subject would only
move the same rejection to the output gate, as
[`negative-output-duplicate`](../negative-output-duplicate/)
shows.
