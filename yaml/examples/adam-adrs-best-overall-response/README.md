# Take each subject's best overall response from a prepared ordering

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/adam-adrs-best-overall-response.html)

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
  that record's date; empty when the subject has no record with
  `BORSEQ` equal to `1`.

**Note:** response, rank, and date agree with each other: all three
are empty for a subject with no record with `BORSEQ` equal to `1`,
and the supporting date is never before the randomization date.

**Standard:** ADaM | **Domain:** ADRS
