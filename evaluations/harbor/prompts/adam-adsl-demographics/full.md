Following CDISC ADaM standards, use the provided DM dataset to create an ADSL
dataset with one record per subject.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, SEX, SEXN, RACE, RACEN, AGE, AGEGR1

SEX is the collected sex, and is "U" when no sex was collected. SEXN is 1
for "M", 2 for "F", and 0 for "U". RACEN is 1 for "WHITE", 2 for "BLACK OR
AFRICAN AMERICAN", 3 for "ASIAN", and 4 for "MULTIPLE". RACEN has no value
when no race was collected, and is 99 for any other collected race. The
race mapping matches the collected text exactly, so a value that differs
in case or wording gives 99 too. AGE is the collected age as a whole
number, and has no value when no age was collected. AGEGR1 is "<18" when
AGE is below 18, "18-64" when AGE is at least 18 and below 65, and ">=65"
when AGE is at least 65. AGEGR1 is "UNKNOWN" when AGE has no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adsl.csv.
