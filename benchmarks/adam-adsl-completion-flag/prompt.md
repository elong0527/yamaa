Following CDISC ADaM standards, use the provided ADSL_RAW and DS
datasets to create an ADSL dataset with one record per subject.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, TRTSDT, COMPLFL

COMPLFL is Y when the subject has a disposition event in the study
(FOLLOW-UP) epoch with standardized outcome COMPLETED; N otherwise. A
subject whose records carry only other outcomes, such as ADVERSE
EVENT, and a subject with no disposition record at all, are both N.

The flag answers whether an end-of-study completion record exists, not
whether its collection date was filled in or how an earlier period
ended, so a subject who completed treatment but discontinued the study
is N.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adsl.csv.
