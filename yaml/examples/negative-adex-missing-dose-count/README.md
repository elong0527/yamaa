# ADaM ADEX: reject an administration expansion with no dose count

This example uses a collected exposure record to attempt one row per
administration:

- `EXSEQ` identifies the collected exposure record, and `EXTRT`, `EXDOSE`, and
  `EXDOSU` are the treatment, amount, and unit it reports;
- `ASTDT` and `AENDT` are the first and last day the record covers;
- `ADOSEN` numbers the administrations represented by the record.

A missing count says neither that no administration occurred nor how many rows
the record represents. The run must fail rather than drop the record or invent
a count.

## How to fix

Decide whether the blank means zero administrations or an unknown count. Record
`0` only for the first policy; otherwise correct the source with the known
number before expansion:

```csv
STUDYID,USUBJID,EXSEQ,EXDOSCNT
PILOT7,P7-504,1,3
```

Do not default every missing count to zero. That would silently erase exposure
whose administration count was not collected.
