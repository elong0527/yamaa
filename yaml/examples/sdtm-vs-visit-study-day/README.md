# SDTM VS visit metadata and study day

This focused collected-to-SDTM probe answers one question: how are visit
metadata and study day attached to a Findings record?

## Rule and record grain

`VS_RAW` is the base, so each collected vital-signs record produces one VS row.
`VISITNUM` and `EPOCH` are looked up from the trial-visits dataset `TV` by the
collected `VISIT` label. `RFSTDTC` is copied from DM by `STUDYID` and
`USUBJID`. `VSDY` applies the SDTM no-Day-0 rule: a date on or after
`RFSTDTC` is `date_diff + 1`, and an earlier date is `date_diff`.

The seven records cover a screening visit before the reference date, a baseline
visit on the reference date, a scheduled post-baseline visit, an unscheduled
visit that `TV` does not define, a collected record with no date, and a subject
with no DM row.

## Two different joins

`TV` shares no column with the output keys, so the R003 automatic left join
cannot reach it and `mapping_from` declares the key instead. DM does share
applicable keys, so `RFSTDTC` uses the R003 join and returns missing for the
unmatched subject.

`mapping_from` returns one column per call, so `VISITNUM` and `EPOCH` are two
separate lookups over the same `TV` row. That repetition is the gap this
fixture names: an expression produces one value, so reading several columns
from one matched record has no expression.

`VISIT`, `VISITNUM`, and `EPOCH` stay separate concepts here: `VISIT` is the
collected label, `VISITNUM` is the ordering value from the trial design, and
`EPOCH` is the design period. Analysis visit windowing (`AVISIT`, `AVISITN`)
belongs to the separate SDTM-to-ADaM boundary and is not part of this fixture.

## The unscheduled visit has no epoch

`EPOCH` is missing for the unscheduled visit. Assigning an epoch to a record
that the trial design does not name requires comparing its date against period
intervals, which needs an interval join the language does not have. The fixture
leaves the value missing rather than inventing terminology.

`RFSTDTC` and `VSDY0` are not SDTM VS variables and declare `output: false`.
Both remain available to predicates and to the `study-day-completeness`
verification.

## Diagnostics and verifications

Expected `VISITNUM.mapping_from.unmapped` and `EPOCH.mapping_from.unmapped`
counts are both one, for the unscheduled visit. No `missing` handler is
declared because `VISIT` is always collected. No conversion-failure handler is
declared.

Rows remain in `VS_RAW` order; the key is `[STUDYID, USUBJID, VSSEQ]`; exactly
seven rows are expected. Visit metadata must be present or absent as a pair,
study day and its intermediate must be present or absent as a pair, no study
day may equal zero, and a record collected on the reference date must be study
day one.
