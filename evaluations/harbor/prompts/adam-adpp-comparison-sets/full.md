Following CDISC ADaM standards, use the provided ADPP_RAW, ADSL, and
COMPARE datasets to create an ADPP dataset with one record per subject,
parameter, and period.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, PARAM, APERIOD, TRTA, TRTSEQP, AVAL, ANL01FL,
ANL02FL

TRTSEQP is the subject's planned sequence of treatments. ANL01FL is "Y"
on the records comparison 1 uses and has no value on every other record,
and ANL02FL is "Y" on the records comparison 2 uses and has no value on
every other record. A record is used by a comparison when its treatment
is one of the comparison's two treatments, its own AVAL is evaluable,
and the same subject has an evaluable AVAL for the same parameter under
the comparison's other treatment. The two treatments of a comparison
are the test treatment and the reference treatment COMPARE lists for
it. An evaluable AVAL is present and greater than zero. A value that is
missing or zero also takes its partner record out of that comparison.
Pairs are judged per parameter, so a subject can be in a comparison for
one parameter and out of it for another. A treatment that is the test
treatment of one comparison and the reference treatment of the other
can carry both flags. Every record stays in the dataset.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adpp.csv.
