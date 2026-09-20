# Reject Conflicting Ages

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-keys-conflict.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmark/README.md#lifecycle)

**Goal:** build one subject-level record carrying the collected age
(`AGE`) and sex (`SEX`), one for each subject the collected
demographics carry.

**Input:** collected demographics records carrying age (`AGE`) and
sex (`SEX`); one subject was collected on two records.

**Variables:**

- `AGE` would be the collected age, carried into the result
  unchanged.
- `SEX` would be the collected sex, carried into the result
  unchanged.

**Note:** the subject decides how many records come out, so a subject
collected twice is still one record and both collected records feed
it. The two records report the same sex, which is one answer given
twice and carries without complaint. They report different ages,
which is two answers to a question the record has one place for, so
the run fails where the age is read and no artifact is accepted.

**Standard:** ADaM | **Domain:** ADSL

## How to fix

Reconcile the two collected records and correct the governed demographics
input so it holds one supported age for the subject. If both records are
legitimate, declare which one answers: drive the specification from a unique
subject inventory and read the demographics record through an ordered
record-selection rule, rather than leaving the choice to source order.

Do not reach for a row template to make the two records two rows. `keys`
states one record per subject, so a second row for that subject would only
move the same rejection to the output gate, as
[`negative-output-duplicate`](../negative-output-duplicate/)
shows.
