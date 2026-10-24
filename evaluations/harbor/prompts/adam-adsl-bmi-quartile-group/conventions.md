Following CDISC ADaM standards, use the provided ADSL dataset to create an ADSL
dataset with one record per subject.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, BMIBL, BMIBLQ1, BMIBLMED, BMIBLQ3, BMIBLGR1

BMIBLQ1, BMIBLMED and BMIBLQ3 follow R's default quantile definition (type
7) and are not rounded. BMIBLGR1 is "Quartile 1", "Quartile 2", "Quartile
3", "Quartile 4" or has no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adsl.csv.
