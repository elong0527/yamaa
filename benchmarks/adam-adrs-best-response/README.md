# Select the Best Overall Response

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/adam-adrs-best-response.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** write one best overall response record for each subject,
carrying the randomization date (`RANDDT`) through and adding the
response (`AVALC`), its rank (`AVAL`), and its supporting date
(`ADT`).

**Input:** subject-level records with randomization date (`RANDDT`),
plus a prepared response selection in which the record with
`BORSEQ` equal to `1` holds the subject's winning category
(`BORCAT`) and date (`ADT`).

**Variables:**

- `AVALC` is the best overall response: complete response (CR),
  partial response (PR), stable disease (SD), neither complete
  response nor progressive disease (NON-CR/NON-PD), progressive
  disease (PD), or not evaluable (NE); empty when the subject has
  no record with `BORSEQ` equal to `1`.
- `AVAL` ranks that response as `1` (complete response), `2`
  (partial response), `3` (stable disease), `4` (neither complete
  response nor progressive disease), `5` (progressive disease), or
  `6` (not evaluable); empty when `AVALC` is empty.
- `ADT` is the analysis date supporting the response, taken from
  that record's date; empty whenever `AVALC` is.

**Note:** every subject in the subject-level records gets one
record, so a subject with no record with `BORSEQ` equal to `1`
keeps its randomization date with response, rank, and date all
empty. The supporting date is never before the randomization
date; it may fall exactly on it.

**Standard:** ADaM | **Domain:** ADRS
