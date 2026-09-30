Following CDISC ADaM standards, use the provided ADSL_RAW, EX, EC, and
FA datasets to create an ADSL dataset with one record per subject.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, DOSADJFL

DOSADJFL is Y when any source reports an adjustment: a filled EXADJ
value, a filled ECADJ value, or a findings-about row with test code
OCCUR, object DOSE ADJUSTMENT, and result Y. A PRESP test code, or a
row about anything but dose adjustment, does not qualify.

DOSADJFL is N when the subject has at least one record in any of the
three sources but no qualifying record. It has no value when the
subject is absent from all three sources.

A source record counts for a subject only when both identifiers
match, and a source record for a subject outside the subject list
adds no record.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adsl.csv.
