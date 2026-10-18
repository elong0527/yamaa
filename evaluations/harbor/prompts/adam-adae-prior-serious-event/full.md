Following CDISC ADaM standards, use the provided AE dataset to create an ADAE
dataset with one record per adverse event.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, AESEQ, AEDECOD, AESER, PRIOR_SAEFL, PRIOR_SAEDECOD,
PRIOR_SAESEQ, FIRST_SAEDECOD, FIRST_SAESEQ, PREV_AEDECOD

AESER is "Y" on a serious event and "N" on a non-serious event. On a
serious event, PRIOR_SAEFL, PRIOR_SAEDECOD, PRIOR_SAESEQ,
FIRST_SAEDECOD, and FIRST_SAESEQ all have no value. On a non-serious
event, PRIOR_SAEFL is "Y" when the subject has a serious event with a
smaller AESEQ, and it has no value when the subject has no such event.
PRIOR_SAEDECOD and PRIOR_SAESEQ are the AEDECOD and the AESEQ of the
subject's most recent earlier serious event, the earlier serious event
with the largest AESEQ, and they have no value when the subject has no
earlier serious event. FIRST_SAEDECOD and FIRST_SAESEQ are the AEDECOD
and the AESEQ of the subject's first serious event, the serious event
with the smallest AESEQ, and they have no value when the subject has
no serious event. A serious event counts as an earlier serious event
and as a first serious event even when its AEDECOD has no value:
PRIOR_SAEFL is still "Y", PRIOR_SAESEQ and FIRST_SAESEQ still name its
AESEQ, and PRIOR_SAEDECOD and FIRST_SAEDECOD have no value for such an
event. PREV_AEDECOD is the AEDECOD of the subject's event with the
next smaller AESEQ, whether that event is serious or not; it has no
value on the subject's event with the smallest AESEQ, and it has no
value when the event with the next smaller AESEQ has no AEDECOD.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adae.csv.
