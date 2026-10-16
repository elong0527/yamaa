Following CDISC ADaM standards, use the provided DM, TA and TE
datasets to create an ADSL dataset with one record per subject.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, TRT01P, TRT02P, TRTSEQP

TRT01P is the name of the first treatment element of the subject's
planned arm, and TRT02P is the name of the second. The planned arm is
the arm in TA with the subject's planned arm code from DM, its
elements are taken in planned order, and only elements in an epoch
whose name begins with TREATMENT are counted, so screening, washout
and follow-up elements are skipped. An element's name is the element
name in TE for its element code. Each period treatment is Vitamin D3
or Placebo.

TRTSEQP is TRT01P and TRT02P joined by "/" in period order. The
planned values follow the planned arm code only, never the actual
arm or the arm description. A subject with no planned arm code has
no value for TRT01P, TRT02P or TRTSEQP.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adsl.csv.
