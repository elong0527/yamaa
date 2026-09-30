Following CDISC ADaM standards, use the provided ADSL_RAW, CM, and PR
datasets to create an ADSL dataset with one record per subject.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, TRTSDT, NACTDT, NACTDY, NACTFL

NACTDT is the earlier of the first qualifying medication start (a
record with category ON TREATMENT) and the first qualifying procedure
start (a record with category CANCER RELATED and subcategory ON
TREATMENT). It has no value when the subject started no qualifying
therapy during the study.

A qualifying therapy must start on or after the treatment start date:
a record dated before it, a PRIOR TREATMENT record, and a procedure
recorded for reasons other than the cancer never qualify, and a
qualifying record with no start date is skipped. A therapy starting on
the treatment start date itself counts.

Read the medication start from the completed start date, which fills a
year-month start to the 15th of that month; a full date passes through
unchanged.

NACTDY is the study day of NACTDT counted from TRTSDT, with the
treatment start date itself as day 1; it has no value when NACTDT has
none. NACTFL holds Y when NACTDT is present and has no value
otherwise.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adsl.csv.
