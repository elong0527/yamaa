Following CDISC ADaM standards, use the provided ADSL, QS, and DS datasets
to flag QS deteriorations (one record per assessment) and create an ADTTE
dataset for time to PRO deterioration, one record per subject.

The flagged QS dataset should contain the following columns in this order:
STUDYID, USUBJID, QSSEQ, AVISIT, ADT, AVAL, DETERFL. The ADTTE dataset should
contain the following columns in this order: STUDYID, USUBJID, PARAMCD, PARAM,
STARTDT, ADT, AVAL, CNSR, EVNTDESC, CNSDTDSC, SRCDOM, SRCVAR, SRCSEQ.

Use PARAMCD "TTDGHS" and PARAM "Time to Deterioration in Global Health
Status". A deterioration is dated after the earliest assessment and scores
at least 10 points below it. The event is the first deterioration, or death
without a prior one. Progression, discontinuation, and withdrawal are
censoring reasons, never events. AVAL counts whole calendar months from
STARTDT to ADT; CNSR is 0 for an event and 1 otherwise. EVNTDESC is PRO
DETERIORATION, DEATH, or CENSORED; CNSDTDSC holds the censoring reason or
has no value. Trace ADT as QS/ADT/sequence, ADSL/DTHDT, or ADSL/RANDDT.

Read the source datasets from /app/input and save the completed datasets as
/app/output/qs_flagged.csv and /app/output/adtte.csv.
