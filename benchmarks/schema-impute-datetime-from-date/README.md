# Treatment Start and End Datetimes From Dates

**Goal:** one row per subject with the datetimes of first (`TRTSDTM`) and last
(`TRTEDTM`) exposure to treatment, built from the treatment start and end
dates (`TRTSDT`, `TRTEDT`), and a flag (`TRTSTMF`) marking that the start's
time of day was supplied.

**Input:** one `spec.yaml` over subject-level records carrying the study, the
subject and the treatment start and end dates, held as dates rather than as
collected text. A subject may still be on treatment, with no end date, or
never treated, with neither date.

**Variables:**

- `TRTSDTM` is the first moment of the start date, `00:00:00`. A subject with
  no start date has no start datetime.
- `TRTSTMF` is `H` when the start datetime exists: its hour, minute and second
  were all supplied, because a date carries no time of day. It is empty when
  there is no start datetime.
- `TRTEDTM` is the last moment of the end date, `23:59:59`, so a comparison
  against it includes the whole last day of treatment. A subject with no end
  date has no end datetime.

**Note:** a date becomes a datetime only by naming which moment of the day is
meant; nothing else about the date changes.
