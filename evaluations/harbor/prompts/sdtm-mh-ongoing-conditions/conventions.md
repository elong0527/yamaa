Following CDISC SDTM standards, use the provided MH_FORM dataset to create
an MH dataset with one record per reported condition.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, MHSEQ, MHTERM, MHSTDTC, MHENDTC, MHENRTPT, MHENTPT

MHENRTPT is ONGOING, BEFORE, or has no value. MHENTPT is SCREENING or has no
value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/mh.csv.
