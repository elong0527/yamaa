Following CDISC ADaM standards, use the provided SDTM VS and SV
datasets to create an ADVS dataset with one record per measurement,
plus one expected record for each planned visit whose window holds no
measurement. Derive PARAMCD from VSTESTCD, ADT from VSDTC, ADY from
VSDY, and AVAL from VSSTRESN; then window on ADY as specified below.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, VSSEQ, VISIT, VISITNUM, ADT, ADY, AVAL,
AVISIT, AVISITN, ANL01FL

AVISIT is the analysis visit whose window holds the record's study
day: SCREENING before day 0, BASELINE on day 1, WEEK 2 on days 2
through 21, WEEK 4 on days 22 through 42, and POST-TREATMENT on day 43
onward with no upper bound. It has no value when the study day is
missing, so such a record belongs to no window. Study days skip from
day -1 to day 1.

AVISITN is the numeric order of the analysis visit: -1 for SCREENING,
0 for BASELINE, 2 for WEEK 2, 4 for WEEK 4, and 99 for
POST-TREATMENT; it has no value when there is no window.

ANL01FL is Y on the record that represents its study, subject,
parameter, and analysis visit: the earliest by study day, with the
lower sequence number breaking a tie on the same day. It has no value
on every other record, including any record with no study day.

A planned visit whose window holds no measurement still appears as an
expected record, so summaries see a complete visit spine. SV lists
each subject's planned visits, including those that did not take
place, and the unscheduled visits. Each planned SCREENING, BASELINE,
WEEK 2, or WEEK 4 visit gets one SYSBP expected record when no
collected record's study day falls in the analysis window of that
name; no other visit gets one. An expected record carries the planned
visit name and number and the window's AVISIT and AVISITN, has no
date, study day, or value, and never takes ANL01FL, even when it is
the only record in its window. Expected records continue the
subject's sequence numbering after the highest collected VSSEQ, in
VISITNUM order.

Windows follow the study day rather than the collected visit name, so
a record left unscheduled still belongs to whichever window its day
falls in. Each window starts with its first day and ends before the
next window's first day, so day 22 opens Week 4 and day 43 opens
post-treatment.

Read the source datasets from /app/input and save the completed dataset as
/app/output/advs.csv.
