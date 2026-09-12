# ADaM ADRS: prepare assessments for best overall response

This example uses the overall response assessments to produce one record per
assessment for the best-response endpoint:

- `ADT` is the assessment date;
- `RANDDY` is the assessment day relative to randomization;
- `AVALC` is the collected overall response and `BORCAT` is the category the
  record can support in the best-response decision. Stable disease and
  neither-complete-nor-progressive disease become not evaluable before day 42;
- `BORPRI` orders the supported categories from complete response through not
  evaluable;
- `BORSEQ` orders each subject's records by that category, assessment date,
  and assessment sequence. The record numbered `1` supplies the subject's best
  overall response and its supporting date.

The ordering fields make the clinical priority an independently testable data
contract. A downstream endpoint can select the first prepared record without
repeating the priority rules.

A record whose collected response is neither one of those categories nor
collected at all supports no category and takes no priority, so it is left
out of the ordering entirely: it can never be numbered `1`, and it never
consumes a number that a usable record would otherwise take.

Subject identifiers are unique only within a study. The sample reuses one
under a second study, and each study's records are ordered on their own, so
one study's numbering never continues into the other's.
