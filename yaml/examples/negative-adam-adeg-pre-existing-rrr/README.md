# ADaM ADEG: reject a collected RR interval

This example uses ADEG HR records and one collected RRR record to attempt to
preserve collected records and add one RRR parameter record per subject and
analysis visit:

- `PARAMCD` is the source parameter code or `RRR` for an added record;
- `PARAM` is the source parameter name or the rederived RR duration name;
- `AVAL` is the source result or 60000 divided by HR for an added `RRR` record.
  A missing or zero HR adds no `RRR` record;
- `AVALU` is the source unit or `ms` for an added `RRR` record.

An RRR record must be produced from HR rather than accepted as collected, so
the run must fail. The expected output records the completed dataset presented
to the failing check.

## How to fix

Remove the collected `RRR` record and retain its contributing `HR` record with
`AVALU: beats/min`. The specification will then calculate the RR interval and
mark it as produced by the calculation.
