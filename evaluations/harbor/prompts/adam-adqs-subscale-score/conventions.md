Following CDISC ADaM standards, use the provided QS dataset to create an ADQS
dataset with one record per subject per visit per parameter.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, VISIT, PARAMCD, PARAM, AVAL, AREASND

AVAL on an item record is the numeric result on the zero to four answer
scale. On the score record, PARAMCD is "PFSCORE" and PARAM is "Physical
Functioning Subscale Score". AVAL is not rounded. AREASND holds a
recorded reason, or is "NOT CALCULABLE", or has no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adqs.csv.
