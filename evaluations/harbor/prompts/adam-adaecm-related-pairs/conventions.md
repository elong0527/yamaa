Following CDISC ADaM standards, use the provided AE, CM, and RELREC datasets to
create an ADAECM dataset with one record per adverse event and medication that
a relationship relates.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, AESEQ, AETERM, CMSEQ, CMTRT, RELID

RELID is the relationship identifier as RELREC writes it, and AESEQ and CMSEQ
are whole numbers.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adaecm.csv.
