Following CDISC ADaM standards, use the provided RS and ADSL datasets
to create an ADRS dataset with one record per investigator overall
response assessment.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, RSSEQ, PARAMCD, PARAM, RSDTC, ADT, ADY, AVALC, AVAL,
ANL01FL

Only assessments the investigator scored as overall response leave a
record; a target-lesion assessment and an independent assessor
assessment leave none. Use PARAMCD "OVR" and PARAM
"Overall Response by Investigator".

RSDTC is the collected assessment date as recorded. ADT is the
completed analysis date: a fully collected date is used as it stands,
a year and month without a day complete to the first of the month, and
a year alone to 1 January. A completed date counts exactly as a
collected one in every comparison that follows. ADY is the study day
of the assessment, counting the treatment start as day one, so an
assessment dated exactly on the treatment start lands on day one.

AVALC is the collected response: complete response (CR), partial
response (PR), stable disease (SD), neither complete response nor
progressive disease (NON-CR/NON-PD), progressive disease (PD), or not
evaluable (NE). AVAL ranks the response from best to worst as 1 (CR),
2 (PR), 3 (SD), 4 (NON-CR/NON-PD), 5 (PD), or 6 (NE).

ANL01FL is Y for one record at each assessment date: the worst
(largest AVAL) response that day, with the lowest RSSEQ breaking ties
when a day carries the same worst response twice. It has no value
otherwise.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adrs.csv.
