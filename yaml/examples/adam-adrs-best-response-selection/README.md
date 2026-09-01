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
