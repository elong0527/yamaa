Following CDISC ADaM standards, use the provided AE dataset to create
an ADAE dataset with one record per adverse event.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, AESEQ, AEDECOD, AESER, PRIOR_SAEFL, PRIOR_SAEDECOD,
PRIOR_SAESEQ, FIRST_SAEDECOD, FIRST_SAESEQ, PREV_AEDECOD

PRIOR_SAEFL is Y on a non-serious event when the subject had a serious
event with a smaller sequence number, and has no value otherwise.
PRIOR_SAEDECOD and PRIOR_SAESEQ name the term and sequence number of
that most recent earlier serious event, and have no value when there
is none. FIRST_SAEDECOD and FIRST_SAESEQ name the term and sequence
number of the subject's first serious event, and have no value when
the subject had no serious event. A serious event leaves its own prior
and first serious event fields empty. A serious event with no coded
term still counts: the prior flag is Y and its sequence number is
named, while the term fields stay empty.

PREV_AEDECOD names the term of the subject's event with the next
smaller sequence number, serious or not. It has no value on the
subject's first event, and when the preceding event has no coded term.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adae.csv.
