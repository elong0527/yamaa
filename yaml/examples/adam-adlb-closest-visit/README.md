# ADaM ADLB closest-to-target record selection

This focused probe answers one question: can the record closest to a target
study day be selected, and is the tie rule expressible?

`../adam-advs-analysis-visit` records closest-to-target selection as a gap and
commits the first-in-window answer instead. This fixture shows the selection is
expressible, at a cost worth naming.

## Rule and input boundary

The input is a small pre-derived ADLB slice carrying `ADT` and `ADY`. One
analysis window is defined: `WEEK 2` spans study days 8 through 22 with target
day 15. Records outside the window are not eligible. Within the window, the
record whose day is nearest the target is the analysis record, and when two are
equally near, the later one wins.

The six records cover a tie with one record before and one after the target, a
record outside the window, an unambiguous nearest record on the far side of the
target, and a record on the lower window boundary.

## How the selection is built

`AWTARGET` carries the target day as data and `ADIST` reads it, so the distance
is one expression, `ABS(ADY - AWTARGET)`, and the two cannot disagree. No guard
is needed: `AWTARGET` is missing outside the window, and `NULL` propagates.

`ARNK` declares `filter: "AVISIT IS NOT NULL"` so only in-window records are
ranked, then orders by nearest distance, `{variable: ADY, direction: desc}` for
the later record, and `LBSEQ` as the final tie-breaker so the result is total.
`ANL01FL` flags rank one.

For subject `CATH-UCSD-0001`, days 10 and 20 are both five days from the
target. The rule selects day 20, and the golden output shows `ARNK = 1` on the
later record. This is the case `../adam-advs-analysis-visit` gets wrong by
design, and the difference between the two fixtures is the whole point.

## Two gaps this fixture names

**The window bounds cannot be data.** `AVISIT` tests `ADY >= 8 AND ADY <= 22`
against literals inside a predicate, so a second window needs its own `case`
branch and the branch count grows with the number of windows. A window, its
bounds, and its target are one concept in the protocol and two unrelated pieces
of specification here — `AWTARGET` is a column, the bounds are not.

**Selection is not auditable as one object.** The reason a record was chosen is
spread across columns rather than carried by the selection itself. `AWTARGET`
and `ADIST` stay in the artifact as the audit trail a reviewer needs, while
`ARNK` is internal mechanism and declares `output: false`. They are the right
columns to publish, but they are still columns rather than one construct.

## Diagnostics and verifications

No handler path is declared. Rows remain in source order; window and ranking
expressions assign values without reordering.

The exact key is `[STUDYID, USUBJID, PARAMCD, LBSEQ]`, and exactly six rows are
expected. `AVISIT`, `AWTARGET`, and `ADIST` must be present or absent together,
and a flagged record must lie inside the window.
