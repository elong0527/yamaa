# ADaM ADLB closest-to-target record selection

This focused probe answers one question: can the record closest to a target
study day be selected, and is the tie rule expressible?

`../adam-advs-analysis-visit` recorded closest-to-target selection as a gap and
committed the first-in-window answer instead. This fixture returns to that gap
and shows that the selection is expressible, at a cost worth naming.

## Rule and input boundary

The input is a small pre-derived ADLB slice carrying `ADT` and `ADY`. One
analysis window is defined: `WEEK 2` spans study days 8 through 22 with target
day 15. Records outside the window are not eligible. Within the window, the
record whose day is nearest the target is the analysis record, and when two are
equally near, the later one wins.

The six records cover a tie with one record before and one after the target, a
record outside the window, an unambiguous nearest record on the far side of the
target, and a record on the lower window boundary.

## How the distance is built

The distance is one expression, `ABS(ADY - AWTARGET)`. One workaround remains:

- `NEGADY` is `-ADY`, existing only so that `row_number.order_by`, which is
  ascending with no direction option, can prefer the later record on a tie.

Before R010 the distance took two more columns. `DIST0` was `add` with an
`addend` of `-15`, because `subtract` typed both operands as variables and
could not take the literal target, and `ADIST` was a `case` that multiplied a
negative `DIST0` by `-1` — the absolute value, spelled out. Both are gone.
`ADIST` also needs no guard: `AWTARGET` is missing outside the window, and
`NULL` propagates.

`ARNK` then orders by `[ADIST, NEGADY, LBSEQ]`: nearest first, later date next,
and sequence as the final tie-breaker so the result is total. `ANL01FL` flags
rank one inside the window.

For subject `CATH-UCSD-0001`, days 10 and 20 are both five days from the
target. The rule selects day 20, and the golden output shows `ARNK = 1` on the
later record. This is the case `../adam-advs-analysis-visit` gets wrong by
design, and the difference between the two fixtures is the whole point.

## The target is written once

`AWTARGET` carries the target day as data, and `ADIST` now reads it:
`ABS(ADY - AWTARGET)`. The two can no longer disagree.

This closes the complaint this fixture originally recorded, that `15` appeared
both as a column and as a literal with nothing keeping them consistent, because
`add.addend` and `multiply.factor` were literal floats and never variables.
`compute` accepts a column in every operand position.

The window bounds are not closed. `AVISIT` still tests `ADY >= 8 AND ADY <= 22`
against literals inside a predicate, so a second window still needs its own
`case` branch and the branch count still grows with the number of windows. A
window, its bounds, and its target remain one concept in the protocol and two
unrelated pieces of specification here.

## Status and named gaps

This fixture is a **probe**. It passes, and it names four gaps.

1. **Closed: no absolute value and no literal subtrahend.** A date distance
   took three columns to express; under R010 it is one expression.
2. **`order_by` has no direction.** Any preference for a later or larger value
   requires a negated companion column. This works for numbers and has no
   equivalent for dates or strings, so a preference rule over a date column
   would need the day number to exist first.
3. **Partly closed: arithmetic operands can now be variables**, so the target
   day is read from `AWTARGET`. The window bounds still cannot be, because
   `cut.breaks` and predicate literals are not variables.
4. **Selection is not auditable as one object.** The reason a record was
   chosen is still spread across five columns rather than carried by the
   selection itself. `output: false` lets the author split them: `AWTARGET`
   and `ADIST` stay in the artifact as the audit trail a reviewer needs, while
   `NEGADY` and `ARNK` are internal mechanism. Two of the five columns this
   fixture once needed are gone, but the rest are still unrelated columns
   rather than one construct.

## Diagnostics and verifications

No handler path is declared. Rows remain in source order; window and ranking
expressions assign values without reordering.

The exact key is `[STUDYID, USUBJID, PARAMCD, LBSEQ]`, and exactly six rows are
expected. The window columns must be present or absent together, `ADIST` must
never be negative, and a flagged record must lie inside the window.
