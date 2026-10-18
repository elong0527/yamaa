Following CDISC ADaM standards, use the provided SV and VS datasets to create
an ADVS dataset with one record per vital signs record, plus an expected
record for each planned visit whose window holds no measurement.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, VSSEQ, VISIT, VISITNUM, ADT, ADY, AVAL, AVISIT,
AVISITN, ANL01FL

PARAMCD is the test code from VS, ADT is the collection date, ADY is the
study day, and AVAL is the numeric result. AVISIT is the analysis visit
whose window holds the record's study day: "SCREENING" before day 0,
"BASELINE" on day 1, "WEEK 2" on days 2 through 21, "WEEK 4" on days 22
through 42, and "POST-TREATMENT" on day 43 onward with no upper bound.
AVISIT has no value when the study day has no value, so such a record
belongs to no window. Windows follow the study day rather than the
collected visit name, so a record the site left unscheduled still
belongs to whichever window its day falls in. AVISITN is -1 for
"SCREENING", 0 for "BASELINE", 2 for "WEEK 2", 4 for "WEEK 4", and 99
for "POST-TREATMENT", and has no value when there is no window.
ANL01FL is "Y" on the record that represents its study, subject,
parameter, and analysis visit: the earliest by study day, with the
lower sequence number breaking a tie on the same day. ANL01FL has no
value on every other record, including any record with no study day,
and an expected record never takes the flag, even when it is the only
record in its window. Add an expected record for each planned visit
named "SCREENING", "BASELINE", "WEEK 2", or "WEEK 4" whose window holds
no measured study day for the subject, whether or not the visit took
place. Unscheduled visits and planned visits after "WEEK 4" never get
an expected record. An expected record carries the planned visit and
its number as VISIT and VISITNUM, "SYSBP" as PARAMCD, and the analysis
visit of the same name as AVISIT with its AVISITN, but no date, day,
or value. An expected record's VSSEQ is the subject's largest
collected VSSEQ plus its place among the subject's expected records
in planned visit number order, counting from 1.

Read the source datasets from /app/input and save the completed dataset as
/app/output/advs.csv.
