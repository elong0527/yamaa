# ADaM ADEX: reject one record per administration built from an aggregate dose

This example uses collected exposure records to attempt one row per
administration:

- `EXSEQ` identifies the collected record an administration came from, and
  `EXTRT`, `EXDOSE`, and `EXDOSU` are the treatment given, the amount given at
  each administration, and the unit it is measured in;
- `ASTDT` and `AENDT` are the first and last day the record covers, repeated
  on every administration taken from it;
- `NDOSE` is how many administrations the record stands for, and `ADOSEN`
  numbers them from one, so a record covering five days is meant to leave five
  rows numbered one to five.

How many administrations a record holds is a property of that record, and a
run can only build as many rows per record as were written out in advance.
A record holding more administrations than that loses the ones past the last
written, so the run must fail and no artifact is accepted. The expected output
records the rows the rejected run built.

## How to fix

The administration grain belongs in the input data. Expand the aggregate
record into one collected record per administration upstream, and read those
records here one to one. For expected-but-uncollected rows, use the long-form
planning input in `adam-advs-once-measured-carry-forward` and enrich it from
collected data.

If the analysis genuinely needs only the totals, drop the
per-administration grain and key on the collected record instead:

```yaml
keys: [STUDYID, USUBJID, EXSEQ]
```

Do not keep the grain and widen the written-out rows to whatever the current
extract needs. It answers correctly only for data that has already been seen,
and the next extract with a longer record silently loses administrations
again.
