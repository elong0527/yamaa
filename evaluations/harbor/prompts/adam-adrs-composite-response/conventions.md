Following CDISC ADaM standards, use the provided ADRS_RAW and ADSL datasets to
create an ADRS dataset with one record per subject and analysis visit.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, PARAM, AVISIT, PCHG, SAEFL, DCSREAS, AVALC, ARSN,
AVAL

PARAMCD is "RESP75" and PARAM is "EASI-75 Response" on every record.
AVALC is "RESPONDER", "NON-RESPONDER", or "NOT EVALUABLE". ARSN is
"THRESHOLD MET", "THRESHOLD NOT MET", "SAFETY OR DISCONTINUATION RULE",
or "COMPONENT MISSING". AVAL is 1 for "RESPONDER", 0 for
"NON-RESPONDER", or has no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adrs.csv.
