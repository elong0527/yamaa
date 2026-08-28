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
cannot reach it. `mapping_from` is the only explicit-key lookup available, and
it returns exactly one column per call, so `VISITNUM` and `EPOCH` need two
separate lookups over the same dictionary row. DM does share applicable keys,
so `RFSTDTC` uses the R003 join and returns missing for the unmatched subject.

`VISIT`, `VISITNUM`, and `EPOCH` stay separate concepts here: `VISIT` is the
collected label, `VISITNUM` is the ordering value from the trial design, and
`EPOCH` is the design period. Analysis visit windowing (`AVISIT`, `AVISITN`)
belongs to the separate SDTM-to-ADaM boundary and is not part of this fixture.

## Status and named gaps

This fixture is a **probe**. It uses only registered constructs, and it makes
three gaps visible.

1. `EPOCH` is missing for the unscheduled visit. Assigning an epoch to a record
   that the trial design does not name requires comparing its date against
   period intervals, which needs an interval join the language does not have.
   The fixture leaves the value missing rather than inventing terminology.
2. `RFSTDTC` and `VSDY0` are not SDTM VS variables and declare
   `output: false`, so a VS artifact no longer carries a DM reference date and
   a raw day count. Both are still available to predicates and to the
   `study-day-completeness` verification.
3. Closed. `VSDY` used `add`, which returned a float, while SDTM
   `--DY` is integral, so the column depended on exact float-to-int conversion
   under R005's unresolved conversion matrix. `compute` with `VSDY0 + 1`
   returns an integer instead: R010 promotes `int + int` to `int`, and `VSDY0`
   is the integer result of `date_diff`. No conversion is involved.

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
