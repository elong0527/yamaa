Following CDISC ADaM standards, use the provided ADRS_RAW dataset to
create an ADRS dataset with one record per collected overall response
assessment.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, ASEQ, ADT, RANDDY, AVALC, BORCAT, BORPRI, BORSEQ

BORCAT is the response category the record can support: complete
response as CR, partial response as PR, stable disease as SD, neither
complete response nor progressive disease as NON-CR/NON-PD,
progressive disease as PD, or not evaluable as NE. Stable disease and
neither-complete-nor-progressive disease count only on or after day 42
after randomization; earlier ones, or ones with no day, fall back to
not evaluable. Any other collected value, including a missing one,
supports no category, so BORCAT has no value.

BORPRI orders the supported categories as 1 (complete response), 2
(partial response), 3 (stable disease), 4
(neither-complete-nor-progressive disease), 5 (progressive disease), 6
(not evaluable); it has no value when the record supports no category.

BORSEQ numbers the usable records of each study and subject in
category order, then by analysis date, then by assessment sequence.
The record numbered 1 supplies the study-subject's best overall
response and its supporting date. A record that supports no category
takes no priority and no number, so numbering passes over it: it can
never be numbered 1. A record that breaks these pairings stops the
run, and no output is written.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adrs.csv.
