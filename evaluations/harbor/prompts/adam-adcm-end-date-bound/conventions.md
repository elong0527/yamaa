Following CDISC ADaM standards, use the provided CM and ADSL datasets to
create an ADCM dataset with one record per concomitant medication.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, CMSEQ, CMTRT, CMENDTC, AENDT, AENDTF, DTHDT

AENDTF is D or M, or has no value. A supplied end is never placed after
a death that falls inside the collected month or year; a fully collected
end date is kept as collected.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adcm.csv.
