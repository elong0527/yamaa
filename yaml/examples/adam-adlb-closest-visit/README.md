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

There is no absolute value, no subtraction by a literal, and no descending sort.
Each is worked around:

- `DIST0` is `add` with an `addend` of `-15`, because `subtract` types both
  operands as variables and cannot take the literal target.
- `ADIST` is a `case` that multiplies a negative `DIST0` by `-1`. That is the
  absolute value, spelled out.
- `NEGADY` is `ADY` multiplied by `-1`, existing only so that
  `row_number.order_by`, which is ascending with no direction option, can
  prefer the later record on a tie.

`ARNK` then orders by `[ADIST, NEGADY, LBSEQ]`: nearest first, later date next,
and sequence as the final tie-breaker so the result is total. `ANL01FL` flags
rank one inside the window.

For subject `CATH-UCSD-0001`, days 10 and 20 are both five days from the
target. The rule selects day 20, and the golden output shows `ARNK = 1` on the
later record. This is the case `../adam-advs-analysis-visit` gets wrong by
design, and the difference between the two fixtures is the whole point.

## The target is written twice

`AWTARGET` carries the target day as data, so the selection can be audited.
The arithmetic cannot use it: `add.addend` and `multiply.factor` are literal
floats, never variables. The value `15` therefore appears both as a column and
as a literal inside `DIST0`, and nothing keeps the two consistent. A second
window would need its own `case` branch with its own literal, so the branch
count grows with the number of windows rather than with the rule.

This is the same shape as the per-row conversion factor recorded by
`../sdtm-vs-unit-standardization`. A window, its bounds, and its target are one
concept in the protocol and three unrelated pieces of specification here.

## Status and named gaps

This fixture is a **probe**. It passes, and it names four gaps.

1. **No absolute value and no literal subtrahend.** A date distance takes three
   columns to express.
2. **`order_by` has no direction.** Any preference for a later or larger value
   requires a negated companion column. This works for numbers and has no
   equivalent for dates or strings, so a preference rule over a date column
   would need the day number to exist first.
3. **Arithmetic operands cannot be variables.** The target day cannot be read
   from a column, so windows cannot be data.
4. **Selection is not auditable as one object.** The reason a record was chosen
   is spread across `AWTARGET`, `DIST0`, `ADIST`, `NEGADY`, and `ARNK`, all of
   which have to be emitted because named intermediates are unsupported. Five
   of this fixture's fourteen columns exist only to explain one flag.

## Diagnostics and verifications

No handler path is declared. Rows remain in source order; window and ranking
expressions assign values without reordering.

The exact key is `[STUDYID, USUBJID, PARAMCD, LBSEQ]`, and exactly six rows are
expected. The window columns must be present or absent together, `ADIST` must
never be negative, and a flagged record must lie inside the window.
