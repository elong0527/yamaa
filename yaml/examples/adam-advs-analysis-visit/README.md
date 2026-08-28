# ADaM ADVS analysis visit windowing

This focused probe answers one question: how is a record assigned to an
analysis window, and which record in that window is the analysis record?

## Rule and input boundary

The input is a small pre-derived ADVS slice that already carries `ADT` and
`ADY`. Keeping study-day derivation outside this fixture makes the windowing
algorithm visible on its own; the collected-to-SDTM half of the same business
flow is `../sdtm-vs-visit-study-day`.

`AVISIT` cuts `ADY` on ascending day boundaries with `right: false`, so each
window is left-closed and right-open: `ADY < 0` is `SCREENING`, `[0, 2)` is
`BASELINE`, `[2, 22)` is `WEEK 2`, `[22, 43)` is `WEEK 4`, and `ADY >= 43` is
`POST-TREATMENT`. `AVISITN` maps each window to its analysis order. `AWRANK`
ranks records inside a subject, parameter, and window, and `ANL01FL` flags rank
one.

The nine records cover a pre-treatment visit, the baseline day, an unscheduled
visit that falls inside a scheduled window, two records competing for one
window, a visit past the last boundary, and a record with no analysis date.

## Analysis visit is not collected visit

`VISIT` and `VISITNUM` are what the site recorded; `AVISIT` and `AVISITN` are
what the analysis assigns from the observed day. The fixture keeps all four so
the difference is visible in the golden output. Record `VSSEQ = 3` is collected
as `UNSCHEDULED` with no `VISITNUM` and still lands in the `WEEK 2` analysis
window. Record `VSSEQ = 6` is collected as `WEEK 8`, which the window list does
not define, and falls into the open-ended `POST-TREATMENT` interval.

Because SDTM study day has no day zero, the `[0, 2)` baseline window can only
contain `ADY = 1`. The `baseline-window-is-study-day-one` verification asserts
that link rather than leaving it as an implementation convention.

## The gap: first-in-window is not closest-to-target

`ANL01FL` selects the first record in the window by `ADY` and then `VSSEQ`.
That rule is expressible, deterministic, and auditable, but it is not the rule
most studies use. Subject `CATH-UCSD-0001` has `ADY = 8` and `ADY = 15` in a
window whose target day is 15, and first-in-window flags the `ADY = 8` record.

`cut` gives window membership but not a target, and `row_number` orders only by
existing variables, so neither expresses closest-to-target on its own.
`../adam-adlb-closest-visit` does express it, by carrying the target as a
column and computing the distance from it. This fixture keeps the simpler rule
and commits its answer rather than hiding the difference; the two golden
outputs disagree on exactly this case.

`AWRANK` declares `output: false`, so the window ranking stays internal and the
artifact carries only the analysis visit, its numeric companion, and the flag.

## Diagnostics and verifications

Expected `AVISIT.cut.missing` and `AVISITN.mapping.missing` counts are both
one, for the record with no analysis date. `AWRANK` declares
`filter: "AVISIT IS NOT NULL"`, so that record falls outside the window
ranking and receives no rank, and `ANL01FL` tests the rank alone. No other
handler path is declared.

Rows remain in source order; window expressions assign values without
reordering. The exact key is `[STUDYID, USUBJID, PARAMCD, VSSEQ]`, and exactly
nine rows are expected. Analysis visit and its numeric companion must be
present or absent as a pair.
