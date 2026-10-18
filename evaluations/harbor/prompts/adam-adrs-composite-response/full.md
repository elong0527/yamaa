Following CDISC ADaM standards, use the provided ADRS_RAW and ADSL datasets to
create an ADRS dataset with one record per subject and analysis visit.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, PARAM, AVISIT, PCHG, SAEFL, DCSREAS, AVALC, ARSN,
AVAL

PARAMCD is "RESP75" and PARAM is "EASI-75 Response" on every record.
AVALC is "NON-RESPONDER" and ARSN is "SAFETY OR DISCONTINUATION RULE"
when the subject's SAEFL is "Y" or the subject's DCSREAS has a value,
whatever the percent change, even when the percent change has no value.
Otherwise AVALC is "NOT EVALUABLE" and ARSN is "COMPONENT MISSING" when
the percent change has no value. Otherwise AVALC is "RESPONDER" and
ARSN is "THRESHOLD MET" when the percent change is -75 or less.
Otherwise AVALC is "NON-RESPONDER" and ARSN is "THRESHOLD NOT MET".
AVAL is 1 when AVALC is "RESPONDER", 0 when AVALC is "NON-RESPONDER",
and has no value when AVALC is "NOT EVALUABLE".

Read the source datasets from /app/input and save the completed dataset as
/app/output/adrs.csv.
