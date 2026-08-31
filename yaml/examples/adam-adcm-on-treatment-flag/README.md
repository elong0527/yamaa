# ADaM ADCM: flag a medication during treatment

This example uses sample CM and ADSL data and a `yamaa` specification to derive
one row per concomitant medication:

- `CMSEQ` is the medication record sequence number;
- `CMTRT` is the medication name;
- `ASTDT` is the medication start date and is empty when none was collected;
- `AENDT` is the medication end date and is empty for an ongoing medication;
- `TRTSDT` is the subject's treatment start date, carried across from ADSL;
- `TRTEDT` is the subject's treatment end date;
- `ONTRTFL` flags a medication as occurring during the treatment phase when its
  dates overlap the treatment period. A medication ending before treatment
  starts, or starting after treatment ends, is left unflagged. A medication
  with missing start or end dates is assumed to overlap if its known dates do
  not rule overlap out. A medication belonging to a subject with no treatment
  start date is left unflagged. When the treatment end date is missing, the
  treatment period remains open-ended.

A subject with no ADSL record keeps their medications and leaves both treatment
dates empty.
