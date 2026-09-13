# Classify each result, its shift from baseline, and one criterion

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/adam-adlb-shift-and-criteria.html)

**Goal:** derive `ANRIND`, `BASE`, `BNRIND`, `SHIFT1`, `R2BASE`,
`CRIT1`, and `CRIT1FL`: mark where each analysis value sits
against its normal range, how that mark moved since baseline, and
whether the record met one high-result criterion.

**Input:** input records carrying the analysis value `AVAL`, the
normal range limits `ANRLO` and `ANRHI`, and the baseline flag
`ABLFL` (`Y` on the record taken as the baseline for the subject
and parameter).

**Variables:**

- `ANRIND` is the record's own mark: `LOW` below `ANRLO`, `HIGH`
  above `ANRHI`, and `NORMAL` between them; empty when `AVAL`,
  `ANRLO`, or `ANRHI` is missing.
- `BASE` repeats the baseline record's `AVAL` on every record of the
  subject and parameter; empty when no record carries the flag.
- `BNRIND` repeats the baseline record's `ANRIND` the same way;
  empty with no flagged baseline.
- `SHIFT1` joins the baseline mark and the record's own mark, baseline
  first, so a result that stayed normal reads `NORMAL to NORMAL` and
  one that moved out of range reads `NORMAL to HIGH`; empty when
  either mark is missing — the record's own or the baseline's.
- `R2BASE` divides `AVAL` by `BASE`, so the baseline record itself
  reads 1 when the ratio can be computed; empty when `AVAL` is
  missing, when there is no baseline, and when the baseline is zero
  since the multiple is not defined there.
- `CRIT1` states the criterion the record was assessed against, a
  result greater than three times the upper limit of normal (ULN):
  `Result greater than 3 x ULN`; empty when `AVAL` or `ANRHI` is
  missing, since the comparison cannot be made.
- `CRIT1FL` says whether the record met it, `Y` or `N`; empty where it
  could not be assessed, which differs from assessed and not met.

**Note:** the shift joins the baseline mark with the record's own
mark and the ratio rests on the flagged baseline value, while the
mark and the criterion rest on the record's own value and limits;
the criterion text and its flag always arrive together, as do the mark
and the shift. A record that breaks these pairings stops the run, and
no output is written.

**Standard:** ADaM | **Domain:** ADLB
