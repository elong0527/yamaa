Following CDISC ADaM standards, use the provided DM and EX datasets to
create an ADSL dataset with one record per subject.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, TRT01A, TRTSDT, TRTSDTM, TRTSTMF, TRTEDT, TRTEDTM,
TRTETMF, TRTDURD, SAFFL

TRT01A is the treatment in upper case, or the text NOT TREATED.

TRTSTMF and TRTETMF each hold H or have no value.

TRTDURD counts both the first and the last day. SAFFL is Y or N.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adsl.csv.
