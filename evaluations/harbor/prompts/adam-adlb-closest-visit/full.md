Following CDISC ADaM standards, use the provided ADLB_RAW dataset to
create an ADLB dataset with one record per laboratory measurement.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, LBSEQ, ADT, ADY, AVAL, AVISIT, AWTARGET,
ADIST, ANL01FL

AVISIT is WEEK 2 for records with a study day from 8 through 22,
limits included; it has no value when the record falls outside that
window or the study day is missing. AWTARGET is the visit's target
day, day 15. ADIST is how far the record's study day lies from the
target, without direction.

AVISIT, AWTARGET, and ADIST travel together: a record inside the
window carries all three, while a record outside every window or with
a missing study day carries none and is never flagged.

ANL01FL is Y for the record that stands for its subject and parameter
in the visit: the closest to the target, the one with the later study
day when two are equally close, or the one with the lower sequence
number when they share the same day. It has no value on every other
record. All records stay in the output.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adlb.csv.
