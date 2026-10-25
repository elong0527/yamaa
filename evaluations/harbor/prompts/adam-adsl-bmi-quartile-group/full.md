Following CDISC ADaM standards, use the provided ADSL dataset to create an ADSL
dataset with one record per subject.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, BMIBL, BMIBLQ1, BMIBLMED, BMIBLQ3, BMIBLGR1

BMIBLQ1, BMIBLMED and BMIBLQ3 are the first quartile, median and third
quartile of the baseline BMI (BMIBL) values of the study's subjects that have
one. Each holds the same value on every record, including the record of a
subject without a baseline value. The quartiles follow Hyndman and Fan's
type 7 definition: the quantile at probability p is the value at position
(n - 1) * p + 1 of the n values in ascending order, interpolated linearly
between the two neighbouring values when that position is fractional, so a
cutpoint need not be an observed value. A definition that returns an observed
value or the midpoint of two gives other first and third quartiles for these
values, although its median is the same. The quartiles are not rounded.

BMIBLGR1 is "Quartile 1" for a baseline value at or below BMIBLQ1,
"Quartile 2" for one above BMIBLQ1 and at or below BMIBLMED, "Quartile 3" for
one above BMIBLMED and at or below BMIBLQ3, and "Quartile 4" for one above
BMIBLQ3, so a value on a cutpoint falls in the lower group. BMIBLGR1 has no
value for a subject without a baseline value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adsl.csv.
