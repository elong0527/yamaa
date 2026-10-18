Following CDISC ADaM standards, use the provided DM, TA, and TE datasets to
create an ADSL dataset with one record per subject.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, TRT01P, TRT02P, TRTSEQP

TRT01P is the name in TE of the first treatment element of the subject's
planned arm, taking the arm's elements in TA in planned order (TAETORD) and
counting only the elements in an epoch whose name begins with "TREATMENT",
so screening, washout and follow-up elements do not count. TRT02P is the
name in TE of the second treatment element of the planned arm, chosen the
same way. TRTSEQP is TRT01P and TRT02P joined by "/" in period order, such
as "Placebo/Vitamin D3". The planned values follow the planned arm code in
DM only, never the actual arm or the arm description, so a subject given
the other treatment order keeps the planned one. TRT01P, TRT02P and TRTSEQP
have no value when the subject has no planned arm code, as for a screen
failure or a subject never assigned to an arm.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adsl.csv.
