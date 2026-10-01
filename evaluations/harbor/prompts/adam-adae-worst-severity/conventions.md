Following CDISC ADaM standards, use the provided ADAE_RAW dataset to
create an ADAE dataset with one record per adverse event.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, AESEQ, AEBODSYS, AEDECOD, ASTDT, AESEV, TRTEMFL,
AESEVN, AWSEVFL

AESEVN is the numeric rank of AESEV: 1 for MILD through 3 for SEVERE.
AWSEVFL is Y on a flagged event.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adae.csv.
