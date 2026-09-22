# Carry the last observed value across missing visits

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/adam-advs-locf.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

Input: Planned vital-sign assessments with collected values or gaps.

Variables:
- AVISITN identifies the planned analysis visit.
- AVAL preserves a collected value or carries the latest earlier collected
  value within the subject and parameter. Leading gaps stay missing; zero
  is a collected value.
