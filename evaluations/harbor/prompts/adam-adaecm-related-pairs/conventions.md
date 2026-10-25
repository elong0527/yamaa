Following CDISC ADaM standards, use the provided AE, CM, and RELREC datasets to
create an ADAECM dataset with one record per adverse event and medication
related to it, per relationship that relates them.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, AESEQ, AETERM, CMSEQ, CMTRT, RELID

RELID is the identifier of the subject's relationship, exactly as RELREC
holds it.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adaecm.csv.
