Following CDISC ADaM standards, use the provided ADVS_RAW and AWINDOW
datasets to create an ADVS dataset with one record per measurement.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, VSSEQ, VISIT, ADT, ADY, AVAL, AVISIT,
AVISITN, AWTARGET, AWTDIFF, ANL01FL

AVISITN is the order number of the analysis visit whose window holds
the record, matched within the same study: the one window whose range
from its first through its last study day, both ends inclusive,
contains the record's study day. It has no value when no window
contains the day.

AWTARGET is the target day stated by the matched window, with no value
when no window matched. AWTDIFF tells how far the study day falls from
its target, negative before the target and positive after, with no
value when there is no window or no study day. An assigned window
brings its analysis visit, order number, and target day together, so a
record with no window has none of the three.

A record has no window when it has no study day, when its day falls in
a gap between stated ranges, or when its day sits on or past the first
day of a window whose last day was never stated: an absent bound is
not an open-ended one. Window edges are inclusive, so study day 21
closes Week 2 and day 22 opens Week 4.

ANL01FL is Y on the record nearest its window target among records for
the same study, subject, parameter, and analysis visit, with the lower
sequence number breaking a tie; it has no value on every other record,
including any record with no window.

Read the source datasets from /app/input and save the completed dataset as
/app/output/advs.csv.
