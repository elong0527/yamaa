# Serious Event Sequence

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/adam-adae-serious-sequence.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** number each subject's serious events (`AESER` is `Y`) in
onset order as `SERSEQ`, so the earliest serious event carries 1.

**Input:** adverse event records carrying the collection sequence
(`AESEQ`), the serious flag (`AESER`), and the onset date (`ASTDT`,
empty when the onset was never collected).

**Variables:**

- `SERSEQ` holds the running number of the subject's serious events,
  ordered by onset date. Events sharing one onset date follow the
  order of their collection sequence, and a serious event with no
  onset date is numbered last. It stays empty for events that are
  not serious.

**Note:** numbering restarts for each subject, so a subject with no
serious event keeps every record unnumbered.

**Standard:** ADaM | **Domain:** ADAE
