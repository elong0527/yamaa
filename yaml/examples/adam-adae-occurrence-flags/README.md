# ADaM ADAE hierarchical occurrence flags

This focused probe answers one question: which treatment-emergent event is
first for a subject, system organ class, and preferred term?

## Rule and input boundary

The input is a small preclassified ADAE slice containing `TRTEMFL`. Keeping
treatment-emergent classification outside this fixture makes the occurrence
algorithm visible without an ADSL join or date-interval logic.

Treatment-emergent rows receive `TEORD = 0`; all other rows receive `1`. Three
`row_number` expressions then sort by `TEORD`, `ASTDT`, and `AESEQ` within the
subject, SOC, and SOC/PT partitions. A rank-one row is flagged only when its
`TRTEMFL` is `Y`.

The seven rows cover repeated preferred terms, multiple terms within one
subject, nonqualifying rows, and same-day ties resolved by `AESEQ`. Expected
flag counts are two `AOCCFL`, three `AOCCSFL`, and three `AOCCPFL` values.

## Design finding and contract

`row_number` cannot filter records. `TEORD` makes this Boolean eligibility case
expressible by ranking nonqualifying rows later, but it does not replace a
general filtered window. `TEORD` and the three rank variables declare
`output: false` under R005, so the ranking machinery stays out of the artifact
while the three flags remain.

Rows remain in source order; window expressions assign values without
reordering. The exact key is `[STUDYID, USUBJID, AESEQ]`, and exactly seven rows
are expected. No handler path is declared.
