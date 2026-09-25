# reject a medication referring to an absent adverse event

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-relrec-unmatched-ae-reference.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** link medications to treated adverse events through `IDVARVAL`,
`RELTYPE`, and `RELID`, and reject an event number absent from the subject's
adverse event records.

**Input:** adverse events (AE) with collected event numbers and delivered
sequence numbers, plus concomitant medications (CM) with up to two event
numbers entered as treatment reasons.

**Variables:**

- `IDVARVAL` would hold the CM sequence number on the medication row and the
  matched AE sequence number on the event row. An unmatched event number
  has no sequence number to report, so no result is accepted.
- `RELTYPE` would be blank because each row identifies one record.
- `RELID` would identify one medication-event pair, shared by its CM and AE
  rows.

**Standard:** SDTM | **Domain:** RELREC

## How to fix

Check the medication's entered event number 99 against the adverse events
for subject P7-002. Correct the reference if it was entered in error, or
add the missing adverse event with its delivered `AESEQ` if the event was
omitted. Keep the subject and study in the match so an event from someone
else cannot satisfy the reference.
