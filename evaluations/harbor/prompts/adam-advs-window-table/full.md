Following CDISC ADaM standards, use the provided ADVS_RAW and AWINDOW datasets
to create an ADVS dataset with one record per measurement.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, VSSEQ, VISIT, ADT, ADY, AVAL, AVISIT, AVISITN,
AWTARGET, AWTDIFF, ANL01FL

A record's window is the one window of its own study whose range, from
its first study day through its last, both ends inclusive, contains the
record's study day (ADY). AVISIT is the analysis visit of the matched
window, AVISITN is its order number, and AWTARGET is its target day. A
record with no window has no value for AVISIT, AVISITN, or AWTARGET. A
record has no window when it has no study day, when its study day falls
in a gap between stated ranges, or when its study day sits on or past
the first day of a window whose last day was never stated: an absent
bound is not an open-ended one. AWTDIFF is the record's study day minus
its target day, negative before the target and positive after, and has
no value when there is no window or no study day. ANL01FL is Y on the
record nearest its window target among the records for the same study,
subject, parameter, and analysis visit, with the lower VSSEQ breaking a
tie; it has no value on every other record, including any record with
no window.

Read the source datasets from /app/input and save the completed dataset as
/app/output/advs.csv.
