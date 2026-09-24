# Multiple Findings About from ODM

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/sdtm-fa-odm-multitest.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** create one FA record for each collected reaction finding from a
long-form Operational Data Model (ODM) diary, while carrying the reported
reaction name and date from the same diary occurrence.

**Input:** each diary occurrence contains a free-text reaction name and
collection date as sibling items, plus separate occurrence, severity, and
optional diameter items. One reaction can therefore produce several FA
tests. A form repeat number can be reused on another diary day.

**Variables:**

- `FASEQ` numbers the subject's findings by diary day, reaction repeat, and
  test order.
- `FATESTCD` and `FATEST` identify occurrence, severity, or longest diameter
  from the collected result item.
- `FAOBJ` is the reaction name exactly as reported in the sibling item.
- `FACAT` is `REACTOGENICITY` for every diary finding.
- `FAORRES` and `FASTRESC` keep the collected result; `FAORRESU` is `mm` for
  diameter only.
- `FASTAT` is `NOT DONE` for a result item that is present but blank, and
  blank for reported results.
- `FATPT` identifies the diary day, and `FADTC` is its collection date.

**Note:** a reaction without a severity or diameter item produces no such FA
test, while a present item with a blank result still produces its FA record.

**Standard:** SDTM | **Domain:** FA
