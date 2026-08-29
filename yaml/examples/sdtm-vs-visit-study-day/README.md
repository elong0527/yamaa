# SDTM VS: look up visit metadata and derive study day

This example uses collected vital signs with DM and the trial-visits dataset
`TV`, and a `yamaa` specification to:

- copy the collected result, date, and visit label into the VS variables;
- look up `VISITNUM` and `EPOCH` from `TV` by the collected `VISIT` label. An
  unscheduled visit that `TV` does not define leaves both missing, and `EPOCH`
  is verified against its allowed values;
- copy `RFSTDTC` from DM by `STUDYID` and `USUBJID`, which stays internal to
  the derivation. A subject with no DM row gets no reference date;
- derive `VSDY` with `study_day`, which applies the SDTM no-Day-0 rule: the
  reference date is day 1, an earlier date counts back from -1, and a record
  with no date or no reference date has no study day.

The seven records cover a visit before the reference date, one on it, a
scheduled visit after it, an unscheduled visit `TV` does not define, a record
with no date, and a subject with no DM row.

No handler path is declared: `VISIT` is always collected, and the two
`mapping_from` lookups declare `unmapped: null` for the unscheduled visit. The
key is `[STUDYID, USUBJID, VSSEQ]` and exactly seven rows are expected. Study
day is verified never to be zero, and a record collected on the reference date
is verified to be day 1.

## Two kinds of join

`TV` shares no column with the output keys, so the R003 automatic join cannot
reach it and `mapping_from` declares the key instead. DM does share applicable
keys, so `RFSTDTC` uses the automatic join and returns missing for the
unmatched subject.

`mapping_from` returns one column per call, so `VISITNUM` and `EPOCH` are two
separate lookups over the same `TV` row. That repetition is the gap this
fixture names: an expression produces one value, so reading several columns
from one matched record has no expression.

## The unscheduled visit has no epoch

Assigning an epoch to a record the trial design does not name means comparing
its date against period intervals, which needs an interval join the language
does not have. The fixture leaves the value missing rather than inventing
terminology.
