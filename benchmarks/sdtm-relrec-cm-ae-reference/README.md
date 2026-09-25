# link medications to the adverse events they treated

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/sdtm-relrec-cm-ae-reference.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** produce related-record rows with `IDVARVAL`, `RELTYPE`, and
`RELID` from the adverse event numbers entered on medication forms.

**Input:** adverse events (AE) with a collected event number and delivered
sequence number, and concomitant medications (CM) with up to two event
numbers entered as treatment reasons.

**Variables:**

- `IDVARVAL` is the CM sequence number on a medication row and the matched
  AE sequence number on an event row, both written as text. An event number
  that matches no event for the same subject rejects the result.
- `RELTYPE` is blank because each row identifies one record.
- `RELID` identifies one medication-event pair. Its CM and AE rows share
  the value, while distinct pairs have distinct values.

**Note:** matching uses the study and subject as well as the collected event
number. A medication with no event reference contributes no row.

**Standard:** SDTM | **Domain:** RELREC
