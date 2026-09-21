# Derive Text Score

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/adam-adqs-derive-text-score.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** demonstrate the aggregate `derive` step for #704: convert
text questionnaire responses to numbers via a typed binding, then
sum them.

**Input:** questionnaire responses with text results (`QSORRES`)
for the physical functioning scale.

**Variables:**

- `PARAMCD`: `PFSCORE` on the score record.
- `PARAM`: the subscale name.
- `AVAL`: the sum of the numeric item responses, converted from
  text via the `derive` binding's declared `float` type.

**Mechanism:** the `derive` step binds `QSNUM` per record by reading
`QS.QSORRES` and converting to `float`. The reducer then sums
`SUM(QSNUM)`. No `to_number` expression is needed; the binding's
declared type drives the conversion.

**Standard:** ADaM | **Domain:** ADQS
