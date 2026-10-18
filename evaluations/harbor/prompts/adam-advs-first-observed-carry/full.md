Following CDISC ADaM standards, use the provided VS dataset to create an ADVS
dataset with one record per subject per parameter per visit.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, AVISITN, AVAL, BASEVAL

Every record of the VS dataset appears once in the output, including a
visit with no collected result, where AVAL has no value. BASEVAL is the
result from the subject's first visit, by visit number, with a collected
result for the parameter. BASEVAL repeats that result on that visit and
on every later visit. BASEVAL has no value on visits before the first
visit with a collected result, and has no value at any visit for a
subject and parameter with no collected result. The carried value never
crosses subjects or parameters: each subject and parameter carries only
its own first observed result.

Read the source datasets from /app/input and save the completed dataset as
/app/output/advs.csv.
