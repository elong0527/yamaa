# Split Adverse Events at Severity Changes

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/adam-adae-worsening-emergence.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** split each adverse event (AE) into continuous analysis periods
and mark treatment-emergent periods (`TRTEMFL`).

**Input:** AE holds one record per event, with its sequence (`AESEQ`),
term (`AEDECOD`), overall start/end (`AESTDTC`, `AEENDTC`) and maximum
severity/grade (`AESEV`, `AETOXGR`). Findings About (FA) holds severity
(`SEV`) and toxicity-grade (`TOXGR`) findings at onset and each collected
change time (`FADTC`). `FALNKID` matches the event's `AELNKID`. The
subject-level analysis dataset (ADSL) supplies first exposure (`TRTSDTM`),
empty for an untreated subject.

**Variables:**

- `ASEQ` numbers analysis periods within a subject; repeated `AESEQ`
  retains the source event identity and its original values.
- `ASTDTM` starts a period at a collected change time. `AENDTM` is the
  next period's start, or the original end for the last period; an
  ongoing event's last end stays empty.
- `ASEV` and `ATOXGRN` hold the period's severity and numeric toxicity
  grade (1 through 5). Each persists until that dimension changes;
  without an earlier collected value it stays empty.
- `TRTEMFL` is `Y` for events starting at or after first exposure. For
  an ongoing pre-treatment event, it starts at the first post-exposure
  increase over the last pre-treatment severity or grade and remains
  `Y` on subsequent periods. Earlier periods, improvement alone and
  untreated subjects' periods stay empty.

**Note:** findings are collected effective change times, rather than
visit assessments from which a change time must be inferred. Simultaneous
severity and grade changes create one period. Missing comparison values
show no worsening for that dimension. Separate events sharing a term
remain separate. Periods are continuous with end equal to next start.

**Standard:** ADaM | **Domain:** ADAE
