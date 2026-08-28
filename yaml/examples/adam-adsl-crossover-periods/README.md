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

Selecting the treatment given in a period now works the same way. R003 allows
`source.multiple_matches` to declare the same `filter`, so `TRT01A` and
`TRT02A` narrow EX to their own period and then order within it. Each subject
has two administrations in one period, so the ordering is doing real work
rather than picking the only candidate.

`CATH-UCSD-0003` never crossed over. Its period-two right side is empty after
filtering, which R003 treats as an absent match, so `TRT02A` is missing because
the selection found nothing. Nothing guards it and nothing has to agree with
anything else.

An earlier version of this fixture selected the first and last exposure record
over all periods and guarded `TRT02A` on `TR02SDT`, a column derived by a
different mechanism. That workaround was correct only for exactly two periods
and only because an unrelated column happened to be missing. It is recorded
here because the two golden files differ: the old one carried `EXFIRST` and
`EXLAST` as evidence that the selection itself was wrong.

## Status and named gaps

This fixture is a **probe**. One of its three original gaps is closed and two
remain.

1. ~~**Ordered selection cannot be filtered.**~~ Closed. `filter` is now
   declared by `min`, `max`, and `multiple_matches`, so a period-scoped value
   selection is direct.
2. **Period is not a concept.** `APERIOD` is an ordinary column matched by a
   literal in four separate predicates. Adding a period means editing every
   predicate and adding another pair of columns, so the specification grows
   with the design rather than describing it.
3. **`TRTxxA` variables are positional.** Nothing links `TRT02A` to
   `TR02SDT`/`TR02EDT` except naming. A period-aware structure would make the
   grouping checkable.

`WASH0` declares `output: false`. No other intermediate remains: the two
columns that existed to expose the old workaround are gone with it.

## Diagnostics and verifications

Expected `EXFIRST.source.multiple_matches` and `EXLAST.source.multiple_matches`
counts are both two, for the two subjects with more than one exposure record.
No other handler path is declared.

Rows remain in DM order; the key is `[STUDYID, USUBJID]`; exactly three rows
are expected. Every period-two variable must be present or absent together,
period two must start after period one ends, and a crossover must change
treatment.
