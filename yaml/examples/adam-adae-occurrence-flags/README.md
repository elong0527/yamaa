# ADaM ADAE hierarchical occurrence flags

This fixture answers one question: which treatment-emergent event is first for
a subject, system organ class, and preferred term?

## Rule and input boundary

The input is a small preclassified ADAE slice containing `TRTEMFL`. Keeping
treatment-emergent classification outside this fixture makes the occurrence
algorithm visible without an ADSL join or date-interval logic.

Three `row_number` expressions declare `filter: "TRTEMFL = 'Y'"` and sort by
`ASTDT` and `AESEQ` within the subject, SOC, and SOC/PT partitions. A
non-emergent row is outside the filter and receives no rank, so a rank of one
is by itself enough to flag.

The seven rows cover repeated preferred terms, multiple terms within one
subject, nonqualifying rows, and same-day ties resolved by `AESEQ`. Expected
flag counts are two `AOCCFL`, three `AOCCSFL`, and three `AOCCPFL` values.

## Design finding and contract

The eligibility condition is stated once, in the window that depends on it. It
was previously a derived sort column ranking non-emergent rows last, which
could not stand alone: ordering still numbers an excluded row, so every flag
had to repeat `TRTEMFL = 'Y'` beside its rank test. The three rank variables
declare `output: false` under R005, so the ranking machinery stays out of the
artifact while the three flags remain.

Rows remain in source order; window expressions assign values without
reordering. The exact key is `[STUDYID, USUBJID, AESEQ]`, and exactly seven rows
are expected. No handler path is declared.
