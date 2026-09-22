# Carry one observed record to each planned visit

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/adam-advs-locf-record.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

Input: Planned visits and selected or unselected vital-sign observations.

Variables:
- AVISITN identifies the planned analysis visit.
- AVAL is the latest non-missing selected result at or before that visit,
  within the same subject and parameter. With no such result it is missing.
- ADT and QSSEQ are the date and sequence of that same observation. A
  missing date stays missing even when an earlier observation has a date.

Note: Visit number, then source sequence, determines the latest record.
