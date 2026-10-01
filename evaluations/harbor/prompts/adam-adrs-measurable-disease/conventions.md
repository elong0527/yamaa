Following CDISC ADaM standards, use the provided ADSL and TU datasets
to create an ADRS dataset with one record per subject.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, PARAM, AVALC, AVAL

Use PARAMCD "MDIS" and PARAM "Measurable Disease at Baseline".

AVALC is Y or N. AVAL is 1 when AVALC is Y and 0 when AVALC is N.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adrs.csv.
