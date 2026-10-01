Following CDISC ADaM standards, use the provided ADSL and ADRSSEL
datasets to create an ADRS dataset with one record per subject.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, PARAM, RANDDT, AVALC, AVAL, ADT

Use PARAMCD "BOR" and PARAM "Best Overall Response by Investigator".

AVALC is one of complete response (CR), partial response (PR), stable disease
(SD), neither complete response nor progressive disease (NON-CR/NON-PD),
progressive disease (PD), or not evaluable (NE), or has no value.

AVAL ranks the response as 1 (complete response), 2 (partial response), 3
(stable disease), 4 (neither complete response nor progressive disease), 5
(progressive disease), or 6 (not evaluable).

Read the source datasets from /app/input and save the completed dataset as
/app/output/adrs.csv.
