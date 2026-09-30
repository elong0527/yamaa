Following CDISC ADaM standards, use the provided VS dataset to create an
ADVS dataset with one record per subject per parameter per visit.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, AVISITN, AVAL, BASEVAL

AVISITN is the visit number of the measurement. AVAL is the collected
numeric result, and has no value when the visit has none.

BASEVAL is the result from the subject's first visit with a collected
result for the parameter, repeated on that and every later visit; it
has no value on visits before any result was collected. The carried
value never crosses subjects or parameters.

Read the source datasets from /app/input and save the completed dataset as
/app/output/advs.csv.
