# Reject Post-CR PR

**Goal:** carry each collected tumor assessment into an analysis
record holding the assessment date (`ADT`) and the recorded
response (`AVALC`), rejecting a partial response (PR) recorded
directly after a complete response (CR).

**Input:** collected tumor assessments with the assessment date
(`RSDTC`) and the assessed response (`RSSTRESC`), identified by
study, subject, and sequence number.

**Variables:**

- `ADT` is the assessment date, taken directly from `RSDTC`.
- `AVALC` is the response recorded at that assessment, taken
  directly from `RSSTRESC`: `CR`, `PR`, stable disease (`SD`),
  `NON-CR/NON-PD`, progressive disease (`PD`), or not evaluable
  (`NE`).

**Note:** assessments are compared in date order within a subject.
A complete response leaves no measurable disease to respond partly,
so an assessment recording `PR` directly after `CR` for the same
subject is a fault in the collected data: the whole run is rejected
and no artifact is accepted, and the expected output records the
completed rows presented to that check. A partial response that
later becomes complete is ordinary and passes.

## How to fix

Review the response sequence and correct the assessment that is inconsistent
with the study definition. If the protocol genuinely permits a partial
response after a complete response, revise the clinical rule and its check
together; do not remove the check merely to accept an unexplained sequence.
