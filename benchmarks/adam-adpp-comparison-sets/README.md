# Records Each Planned Treatment Comparison Uses

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/adam-adpp-comparison-sets.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** flag, for each planned test-versus-reference comparison of a
three-period crossover, the pharmacokinetic (PK) parameter records the
comparison uses (`ANL01FL`, `ANL02FL`), and carry the planned sequence
(`TRTSEQP`).

**Input:** one record per PK parameter, subject and period with the
actual treatment (`TRTA`) and the value (`AVAL`); the subject-level
dataset with the planned sequence; and a table of the planned
comparisons, each with its number (`COMPNO`), test treatment
(`TESTTRT`) and reference treatment (`REFTRT`).

**Variables:**

- `APERIOD` and `TRTA` are the record's period and the treatment
  actually given in it, unchanged.
- `TRTSEQP` is the subject's planned sequence of treatments.
- `AVAL` is the parameter value, unchanged.
- `ANL01FL` is `Y` on the records comparison 1 uses and empty on every
  other record; `ANL02FL` does the same for comparison 2. A record is
  used when its treatment is one of the comparison's two treatments,
  its own value is evaluable, and the same subject has an evaluable
  value of the same parameter under the comparison's other treatment.
  Evaluable means present and greater than zero.

**Note:** pairs are judged per parameter, so a subject can be in a
comparison for one parameter and out of it for another, and a value
that is missing or zero also takes its partner record out of that
comparison. A treatment that is the test of one comparison and the
reference of the other can carry both flags. Every record stays in the
dataset.

**Standard:** ADaM | **Domain:** ADPP
