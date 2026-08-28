# ADaM ADSL crossover periods

This SDTM-to-ADaM fixture answers one question: can period-scoped treatment and
dates be derived without a subject-level join collapsing the periods?

## Rule and record grain

DM is the base, so each subject produces one ADSL row. Two treatment periods
are separated by a washout. `TR01SDT` through `TR02EDT` reduce EX inside each
period, and `TRT01A` and `TRT02A` name the treatment given in each. `WASHDUR`
is the number of days strictly between the end of period one and the start of
period two.

Two subjects cross over in opposite orders. A third completes only period one,
so every period-two variable must stay missing.

## The aggregate filter is what keeps the periods apart

`min` and `max` narrow EX to one period before the R003 join, so a
subject-level join never collapses period records and each period gets its own
dates. `source.multiple_matches` declares the same `filter`, so `TRT01A` and
`TRT02A` narrow EX to their own period and then order within it. Two subjects
have two administrations inside one of their periods, so the ordering is doing
real work rather than picking the only candidate.

`CATH-UCSD-0003` never crossed over. Its period-two right side is empty after
filtering, which R003 treats as an absent match, so `TRT02A` is missing because
the selection found nothing. Nothing guards it and nothing has to agree with
anything else.

## Two gaps this fixture names

**Period is not a concept.** `APERIOD` is an ordinary column matched by a
literal in four separate predicates. Adding a period means editing every
predicate and adding another pair of columns, so the specification grows with
the design rather than describing it.

**`TRTxxA` variables are positional.** Nothing links `TRT02A` to
`TR02SDT`/`TR02EDT` except naming. A period-aware structure would make the
grouping checkable.

## Diagnostics and verifications

Expected `TRT01A.source.multiple_matches` and `TRT02A.source.multiple_matches`
counts are both one: `CATH-UCSD-0001` has two period-one administrations and
`CATH-UCSD-0002` has two in period two. `CATH-UCSD-0003` has no period-two
record at all, which is an absent match under R003 rather than a handler path.
`WASH0` declares `output: false`.

Rows remain in DM order; the key is `[STUDYID, USUBJID]`; exactly three rows
are expected. Every period-two variable must be present or absent together,
period two must start after period one ends, and a crossover must change
treatment.
