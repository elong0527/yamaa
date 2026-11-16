Following CDISC ADaM standards, use the provided ADSL dataset to create an ADSL
dataset with one record per subject.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, HEIGHTBL, WEIGHTBL, BMIBL

HEIGHTBL is the baseline height in cm and WEIGHTBL the baseline weight in
kg, as given. BMIBL is the baseline body mass index: WEIGHTBL divided by
the square of the height in metres, where the height in metres is
HEIGHTBL divided by 100. BMIBL has no value when HEIGHTBL has no value or
is zero, or when WEIGHTBL has no value. A WEIGHTBL of zero gives a BMIBL
of zero. BMIBL is not rounded.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adsl.csv.
