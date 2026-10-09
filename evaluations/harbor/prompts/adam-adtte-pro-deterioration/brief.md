Following CDISC ADaM standards, use the provided ADSL, QS, and DS datasets
to flag QS deteriorations (one record per assessment) and create an ADTTE
dataset for time to PRO deterioration, one record per subject.

The flagged QS dataset should contain the following columns in this order:
STUDYID, USUBJID, QSSEQ, AVISIT, ADT, AVAL, DETERFL. The ADTTE dataset should
contain the following columns in this order: STUDYID, USUBJID, PARAMCD, PARAM,
STARTDT, ADT, AVAL, CNSR, EVNTDESC, CNSDTDSC, SRCDOM, SRCVAR, SRCSEQ.

Read the source datasets from /app/input and save the completed datasets as
/app/output/qs_flagged.csv and /app/output/adtte.csv.
