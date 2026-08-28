# ADaM ADSL crossover periods

This focused SDTM-to-ADaM probe answers one question: can period-scoped
treatment and dates be derived without a subject-level join collapsing the
periods?

## Rule and record grain

DM is the base, so each subject produces one ADSL row. Two treatment periods
are separated by a washout. `TR01SDT` through `TR02EDT` reduce EX inside each
period, and `TRT01A` and `TRT02A` name the treatment given in each. `WASHDUR`
is the number of days strictly between the end of period one and the start of
period two.

Two subjects cross over in opposite orders. A third completes only period one,
so every period-two variable must stay missing.

## Dates scope cleanly; treatments do not

The aggregate `filter` is what keeps the periods apart. `min` and `max` narrow
EX to one period before the R003 join, so a subject-level join never collapses
period records and each period gets its own dates. This is the part of X06 the
language handles well.

Selecting the treatment given in a period does not work the same way. Ordered
`source.multiple_matches` cannot take a filter, so `TRT01A` and `TRT02A` are
built from a first-and-last selection over all of a subject's exposure records.
That is correct only because there are exactly two periods and they coincide
with first and last. A three-period design could not be written this way at
all.

Worse, the workaround is wrong on its own. `CATH-UCSD-0003` never crossed over,
and its last exposure record is still period one, so `EXLAST` reports
`VITAMIN D3`. `TRT02A` is missing only because a `case` guards it on
`TR02SDT`, a column derived by a completely different mechanism. Correctness
depends on two unrelated derivations agreeing rather than on the selection
itself. `EXFIRST` and `EXLAST` are emitted so this is visible in the golden
output.

## Status and named gaps

This fixture is a **probe**. It names three gaps.

1. **Ordered selection cannot be filtered.** The aggregate `filter` exists for
   `min` and `max` only. Extending it to `source.multiple_matches` would make
   period-scoped value selection direct and would remove the guard.
2. **Period is not a concept.** `APERIOD` is an ordinary column matched by a
   literal in four separate predicates. Adding a period means editing every
   predicate and adding another pair of columns, so the specification grows
   with the design rather than describing it.
3. **`TRTxxA` variables are positional.** Nothing links `TRT02A` to
   `TR02SDT`/`TR02EDT` except naming. A period-aware structure would make the
   grouping checkable.

`EXFIRST` and `EXLAST` are not ADaM variables and are emitted only because
named intermediates are unsupported.

## Diagnostics and verifications

Expected `EXFIRST.source.multiple_matches` and `EXLAST.source.multiple_matches`
counts are both two, for the two subjects with more than one exposure record.
No other handler path is declared.

Rows remain in DM order; the key is `[STUDYID, USUBJID]`; exactly three rows
are expected. Every period-two variable must be present or absent together,
period two must start after period one ends, and a crossover must change
treatment.
