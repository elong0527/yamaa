Following CDISC ADaM standards, use the provided DM and EX datasets to
create an ADSL dataset with one record per subject.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, TR01SDT, TR01EDT, TR02SDT, TR02EDT, TRT01A, TRT02A,
WASHDUR

TR01SDT is the earliest recorded start date among the subject's
exposure records in the TREATMENT 1 epoch; it has no value when the
subject has no such record. TR01EDT is the latest recorded end date
there. TR02SDT and TR02EDT are the same for the TREATMENT 2 epoch.
Every value comes only from exposure records in its own period.

TRT01A is the treatment given on the subject's earliest exposure
record in the TREATMENT 1 epoch, earliest by start date with the lower
sequence number breaking ties on the same day and a record with no
start date sorting last; either VITAMIN D3 or PLACEBO, and empty when
the subject has no period-one exposure. TRT02A is chosen the same way
in the TREATMENT 2 epoch.

WASHDUR is the number of days strictly between the end of period one
and the start of period two, counting neither endpoint, so periods
that touch give zero; it has no value when either date is missing. A
subject with no period-two exposure leaves the period-two dates and
treatment and the washout empty together.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adsl.csv.
