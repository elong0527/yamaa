Following CDISC ADaM standards, use the provided ADSL, AE, and FA datasets to
create an ADAE dataset with one record per subject per adverse event per
analysis period.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, ASEQ, AESEQ, AEDECOD, AESTDTC, AEENDTC, ASTDTM, AENDTM,
TRTSDTM, AESEV, AETOXGR, ASEV, ATOXGRN, TRTEMFL

Severity ranks in the order MILD, then MODERATE, then SEVERE. TRTEMFL is Y
or has no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adae.csv.
